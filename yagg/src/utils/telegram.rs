use log::{info, warn};
use teloxide::Bot;
use teloxide::prelude::{ChatId, Requester};
use crate::models::GradeDiff;

// Constructs a message string from a vector of GradeDiff, indicating new or updated grades.
pub fn parse_new_grades_message(diffs: Vec<GradeDiff>) -> String {
    let mut message = String::from("📚 Nouvelles Notes Disponibles! 📚\n\n");
    for diff in diffs {
        let emoji = if diff.category == "laboratoire" { "🔬" } else { "📖" };
        message.push_str(&format!("{} Nouvelle note de {} en {} : {}\n", emoji, diff.category, diff.course, diff.grade));
    }
    message += "\nGardez le cap sur l'excellence ! 🚀";
    message
}

// Sends the constructed message asynchronously to a specified chat using a bot token.
pub async fn send(message: &str) {
    // Fetches the bot token and chat ID from environment variables
    let bot_token = std::env::var("BOT_TOKEN").expect("BOT_TOKEN environment variable not found");
    let chat_id = std::env::var("CHAT_ID").expect("CHAT_ID environment variable not found");

    let bot = Bot::new(bot_token);

    // Attempts to send the message and logs the outcome
    match bot.send_message(ChatId(chat_id.parse().unwrap()), message).await {
        Ok(message) => info!("Text message sent successfully {:?}", message.id),
        Err(e) => warn!("Text message wasn't sent because of: {}", e)
    };
}
