use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::BTreeMap;
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

// Adjust the diff_grades function to handle multiple occurrences of the same grade.
pub fn diff_grades(fetched_courses: &[Course]) -> Result<Vec<GradeDiff>> {
    let file_path = "grades.json";
    let mut diffs: Vec<GradeDiff> = Vec::new();

    let file_exists_and_non_empty = Path::new(file_path).exists() && fs::metadata(file_path).map(|m| m.len() > 0).unwrap_or(false);

    let previous_courses: Vec<Course> = if file_exists_and_non_empty {
        match fs::read_to_string(file_path) {
            Ok(contents) => serde_json::from_str(&contents)?,
            Err(e) => return Err(anyhow!("Failed to read previous grades: {}", e)),
        }
    } else {
        Vec::new()
    };

    // Convert the list of grades into a map that counts the occurrences of each grade in each category.
    let prev_courses_map = to_grade_count_map(&previous_courses);

	for fetched_course in fetched_courses {
		// fetched_course_map is similarly structured to prev_courses_map but for the current fetched course
		let fetched_course_map = to_grade_count_map(&[fetched_course.clone()]);

		// Iterate over categories and their grades for the fetched course
		for (category, grades) in fetched_course_map.get(&fetched_course.name).unwrap_or(&BTreeMap::new()) {
		    for (grade, &count) in grades {
		        let prev_count = prev_courses_map.get(&fetched_course.name)
		                            .and_then(|categories| categories.get(category))
		                            .and_then(|grades| grades.get(grade))
		                            .copied()
		                            .unwrap_or(0);

		        // Add differences to vector if there is any
                // Also supports the case where we receive two notes with same value, of same category in the same course
                for _ in 0..(count-prev_count) {
                    diffs.push(GradeDiff {
                        course: fetched_course.name.clone(),
                        category: category.clone(),
                        grade: grade.clone(),
                    });
                }
		    }
		}
	}	

    fs::write(file_path, to_string_pretty(&fetched_courses)?)?;

    Ok(if file_exists_and_non_empty { diffs } else { Vec::new() })
}

// Converts a list of courses into a map that counts the occurrences of each grade within each category of each course.
fn to_grade_count_map(courses: &[Course]) -> BTreeMap<String, BTreeMap<String, BTreeMap<String, usize>>> {
    let mut map = BTreeMap::new();
    for course in courses {
        let course_entry = map.entry(course.name.clone()).or_insert_with(BTreeMap::new);
        for grade in &course.grades {
            let category_entry = course_entry.entry(grade.category.clone()).or_insert_with(BTreeMap::new);
            *category_entry.entry(grade.value.clone()).or_insert(0) += 1;
        }
    }
    map
}
