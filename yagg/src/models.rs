use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct GradeDiff {
    pub course: String,
    pub category: String,
    pub grade: String,
    pub average: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Course {
    pub name: String,
    pub grades: Vec<Grade>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Grade {
    pub value: String,
    pub category: String,
    pub average: String,
    pub name: String,
}
