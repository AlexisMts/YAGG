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
        let request_body = [("rs", "smartReplacePart"), ("rsargs", "[\"result\",\"result\",null,null,null,null]")];
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
    let regex_pattern = r#"\+:\"@.*@(.*)@.*@\""#;
    let re = Regex::new(regex_pattern).unwrap();
    let matches = re.captures(&decoded_html).unwrap();
    let mut parsed_html = matches.get(1).map_or("", |m| m.as_str()).to_string();
    parsed_html = parsed_html.replace("\\\"", "\"");
    parsed_html = parsed_html.replace("\\/", "/");
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
                }

                // Extracts and adds grades to the current course.
                let last = if tds.len() > 1 { &tds[tds.len() - 1] } else { &tds[0] };
                if let Some(class_attr) = last.value().attr("class") {
                    if class_attr.contains("bodyCC") {
                        let grade_value = last.text().collect::<Vec<_>>()[0].split_whitespace().next().unwrap_or("-");
                        if grade_value != "-" {
                            let grade = Grade {
                                value: grade_value.to_string(),
                                category: last_category.clone(),
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

// Compares fetched courses and grades with previously stored data to identify any differences.
pub fn diff_grades(fetched_courses: &[Course]) -> Result<Vec<GradeDiff>> {
    let file_path = "grades.json";
    let mut diffs: Vec<GradeDiff> = Vec::new();

    // Checks if the grades file exists and is not empty.
    let file_exists_and_non_empty = Path::new(file_path).exists() && fs::metadata(file_path).map(|m| m.len() > 0).unwrap_or(false);

    // Loads previous courses from the file, if available.
    let previous_courses: Vec<Course> = if file_exists_and_non_empty {
        match fs::read_to_string(file_path) {
            Ok(contents) => serde_json::from_str(&contents)?,
            Err(e) => return Err(anyhow!("Failed to read previous grades: {}", e)),
        }
    } else {
        Vec::new()
    };

    // Identifies new or updated grades compared to previously stored data.
    if !previous_courses.is_empty() {
        for fetched_course in fetched_courses {
            let prev_course = previous_courses.iter().find(|&c| c.name == fetched_course.name);

            for fetched_grade in &fetched_course.grades {
                if let Some(prev_course) = prev_course {
                    let grade_not_found = !prev_course.grades.iter().any(|g| g.value == fetched_grade.value && g.category == fetched_grade.category);

                    if grade_not_found {
                        diffs.push(GradeDiff {
                            course: fetched_course.name.clone(),
                            category: fetched_grade.category.clone(),
                            grade: fetched_grade.value.clone(),
                        });
                    }
                } else {
                    // Considers all grades from new courses as differences.
                    diffs.push(GradeDiff {
                        course: fetched_course.name.clone(),
                        category: fetched_grade.category.clone(),
                        grade: fetched_grade.value.clone(),
                    });
                }
            }
        }
    }

    // Updates the file with the current grades for future comparisons.
    fs::write(file_path, to_string_pretty(&fetched_courses)?)?;

    // Returns identified differences, if any, except for the first run when the file was empty or didn't exist.
    Ok(if file_exists_and_non_empty { diffs } else { Vec::new() })
}
