use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use anyhow::{anyhow, Context};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::{to_string_pretty};
use urlencoding::encode;
use crate::models::{Course, Grade, GradeDiff};
use anyhow::Result;

// Asynchronously retrieves grades from the specified URL, handling login and grade request.
pub async fn retrieve_grades() -> Result<String> {

    let client = Client::builder()
        .cookie_store(true)
        .build()
        .context("Failed to build the client")?;

    let username = encode(std::env::var("GAPS_USERNAME").context("GAPS_USERNAME environment variable not found")?.as_str()).to_string();
    let password = encode(std::env::var("GAPS_PASSWORD").context("GAPS_PASSWORD environment variable not found")?.as_str()).to_string();


    let login_data = HashMap::from([
        ("login", username.as_str()),
        ("password", password.as_str()),
        ("submit", "Entrer"),
    ]);

    let login_response = client.post("https://gaps.heig-vd.ch/consultation/controlescontinus/consultation.php")
        .form(&login_data)
        .body(format!("login={}&password={}&submit=Entrer", username, password))
        .send()
        .await
        .context("Failed to send login request")?;

    if login_response.status().is_success() {
        // Gaps does a first GET request to load the static pages (without the grades)
        // Once the page is ready, it executes a POST request to get the grades
        // We only do the second request, as the first one is not necessary
        let request_body = [("rs", "replaceHtmlPart"), ("rsargs", "[\"result\",null]")];
        let url = "https://gaps.heig-vd.ch/consultation/controlescontinus/consultation.php";
        let grades_response = client.post(url)
            .form(&request_body)
            .send()
            .await
            .context("Failed to send grades request")?;

        Ok(grades_response.text().await.context("Failed to read response text")?)
    } else {
        Err(anyhow::Error::msg("Authentication Failed, check your credentials and try again."))
    }
}

// Parses the HTML response to extract the raw HTML content for further processing.
fn parse_html_response(html_content: &str) -> String {
    let decoded_html = Cow::from(html_content);
    let regex_pattern = r#"\+:"\{\\"parts\\":\{\\"result\\":\\"(.*)\\"}}""#;
    let re = Regex::new(regex_pattern).unwrap();
    let matches = re.captures(&decoded_html).unwrap();
    let mut parsed_html = matches.get(1).map_or("", |m| m.as_str()).to_string();
    parsed_html = parsed_html.replace("\\\\\\\"", "\"");
    parsed_html = parsed_html.replace("\\\\\\/", "/");
    parsed_html
}

// Parses grades from the provided HTML content, organizing them into Course and Grade structures.
pub fn parse_grades(html_content: &str) -> Vec<Course> {
    let html_content = parse_html_response(html_content);
    let document = Html::parse_document(&html_content);
    let table_selector = Selector::parse("table.displayArray").unwrap();
    let tr_selector = Selector::parse("tr").unwrap();
    let td_selector = Selector::parse("td").unwrap();

    let mut courses = Vec::new();
    let mut current_course = Course { name: String::new(), grades: Vec::new() };
    let mut last_category = String::new();

    // Iterates over each row in the table to extract course names and grades.
    if let Some(table) = document.select(&table_selector).next() {
        for tr in table.select(&tr_selector) {
            let tds: Vec<_> = tr.select(&td_selector).collect();
            if !tds.is_empty() {
                let first = &tds[0];
                // Identifies course names and initializes new Course objects.
                if let Some(class_attr) = first.value().attr("class") {
                    if class_attr.contains("bigheader") {
                        if !current_course.name.is_empty() {
                            courses.push(current_course.clone());
                            current_course.grades.clear();
                        }
                        current_course.name = first.text().collect::<Vec<_>>()[0].split_whitespace().next().unwrap_or("").to_string();
                    }
                }

                // Determines the category of each grade based on its description.
                if first.text().collect::<Vec<_>>()[0].contains("Cours") {
                    last_category = "cours".to_string();
                } else if first.text().collect::<Vec<_>>()[0].contains("Laboratoire") {
                    last_category = "laboratoire".to_string();
                } else if first.text().collect::<Vec<_>>()[0].contains("Projet") {
                    last_category = "projet".to_string();
                }

                // Extracts and adds grades to the current course.
                let name_element = if tds.len() > 1 { &tds[1] } else { &tds[0] };
                let average_element = if tds.len() > 1 { &tds[tds.len() - 3] } else { &tds[0] };
                let grade_element = if tds.len() > 1 { &tds[tds.len() - 1] } else { &tds[0] };
                if let Some(class_attr) = grade_element.value().attr("class") {
                    if class_attr.contains("bodyCC") {
                        let name = name_element.text().collect::<Vec<_>>().join(" ");
                        let average_value = average_element.text().collect::<Vec<_>>()[0].split_whitespace().next().unwrap_or("-");
                        let grade_value = grade_element.text().collect::<Vec<_>>()[0].split_whitespace().next().unwrap_or("-");
                        if grade_value != "-" {
                            let grade = Grade {
                                value: grade_value.to_string(),
                                category: last_category.clone(),
                                average: average_value.to_string(),
                                name: name.to_string(),
                            };
                            current_course.grades.push(grade);
                        }
                    }
                }
            }
        }
    }
    if !current_course.name.is_empty() {
        courses.push(current_course);
    }

    courses
}

// Adjust the diff_grades function to handle multiple occurrences of the same grade.
pub fn diff_grades(fetched_courses: &[Course]) -> Result<Vec<GradeDiff>> {
    let file_path = "grades.json";

    let file_exists_and_non_empty = Path::new(file_path).exists() && fs::metadata(file_path).map(|m| m.len() > 0).unwrap_or(false);

    let previous_courses: Vec<Course> = if file_exists_and_non_empty {
        match fs::read_to_string(file_path) {
            Ok(contents) => serde_json::from_str(&contents)?,
            Err(e) => return Err(anyhow!("Failed to read previous grades: {}", e)),
        }
    } else {
        Vec::new()
    };

    let mut grade_diffs: Vec<GradeDiff> = Vec::new();

    // Iterate through the new courses
    for new_course in fetched_courses {
        // Find the corresponding old course by name
        if let Some(old_course) = previous_courses.iter().find(|c| c.name == new_course.name) {
            // Compare grades within the course
            for new_grade in &new_course.grades {
                if let Some(old_grade) = old_course.grades.iter().find(|g| g.name == new_grade.name && g.category == new_grade.category) {
                    // If grades are different, capture the difference
                    if old_grade.value != new_grade.value {
                        grade_diffs.push(GradeDiff {
                            course: new_course.name.clone(),
                            category: new_grade.category.clone(),
                            grade: new_grade.value.clone(),
                            average: new_grade.average.clone(),
                            name: new_grade.name.clone(),
                        });
                    }
                } else {
                    // Grade is new, consider it as a difference
                    grade_diffs.push(GradeDiff {
                        course: new_course.name.clone(),
                        category: new_grade.category.clone(),
                        grade: new_grade.value.clone(),
                        average: new_grade.average.clone(),
                        name: new_grade.name.clone(),
                    });
                }
            }
        } else {
            // Entire course is new, consider all its grades as differences
            for new_grade in &new_course.grades {
                grade_diffs.push(GradeDiff {
                    course: new_course.name.clone(),
                    category: new_grade.category.clone(),
                    grade: new_grade.value.clone(),
                    average: new_grade.average.clone(),
                    name: new_grade.name.clone(),
                });
            }
        }
    }

    fs::write(file_path, to_string_pretty(&fetched_courses)?)?;

    Ok(if file_exists_and_non_empty { grade_diffs } else { Vec::new() })
}