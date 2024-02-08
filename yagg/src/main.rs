mod utils;
mod models;
use dotenv::dotenv;
use log::{error, info};
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};
use crate::utils::gaps::{diff_grades, parse_grades, retrieve_grades};
use crate::utils::telegram::{parse_new_grades_message, send};

// Entry point for the async main function, powered by tokio runtime.
#[tokio::main]
async fn main() {
    // Loads environment variables from a `.env` file, if present.
    dotenv().ok();

    // Initializes logging with simplelog to the terminal with mixed output (both stdout and stderr) and automatic color support.
    TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto
    ).unwrap();

    // Retrieves grades as HTML from the specified source.
    let html = match retrieve_grades().await {
        Ok(html) => {
            info!("Grades retrieved successfully");
            html
        },
        Err(e) => {
            error!("Error retrieving grades: {}", e);
            return;
        },
    };

    // Parses the HTML content to extract grades into structured data.
    let parsed_grades = parse_grades(&html);

    // Compares the newly fetched grades with previously stored ones to identify any differences.
    let new_grades = match diff_grades(&parsed_grades) {
        Ok(diffs) => {
            info!("Grade differences computed successfully");
            diffs
        },
        Err(e) => {
            error!("Error computing grade differences: {}", e);
            return;
        },
    };

    // If there are no new grades, exits the function early.
    if new_grades.is_empty() {
        info!("No new grades found");
        return;
    }

    // Constructs a message from the list of new or updated grades.
    let message = parse_new_grades_message(new_grades);

    // Sends the constructed message via Telegram.
    send(&message).await;
}
