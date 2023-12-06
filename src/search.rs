use reqwest;
use serde_json::Value;

use crate::SupabaseClient;

// Enums for different types of operators and sort orders
pub enum Operator {
    Equals,
    GreaterThan,
    LessThan,
    // ... other operators
}

pub enum SortOrder {
    Ascending,
    Descending,
}

// Struct for a single filter condition
pub struct Filter {
    column: String,
    operator: Operator,
    value: String,
}

// Struct for sorting
pub struct Sort {
    column: String,
    order: SortOrder,
}

// Struct for the main query
pub struct Query {
    filters: Vec<Filter>,
    sort: Option<Sort>,
    limit: Option<u32>,
    offset: Option<u32>,
}

// Implementing the builder pattern for Query
impl Query {
    // Constructor for a new Query
    pub fn new() -> Query {
        Query {
            filters: Vec::new(),
            sort: None,
            limit: None,
            offset: None,
        }
    }

    // Method to add a filter
    pub fn filter(mut self, filter: Filter) -> Query {
        self.filters.push(filter);
        self
    }

    // Method to set sort
    pub fn sort(mut self, sort: Sort) -> Query {
        self.sort = Some(sort);
        self
    }

    // Method to set limit
    pub fn limit(mut self, limit: u32) -> Query {
        self.limit = Some(limit);
        self
    }

    // Method to set offset
    pub fn offset(mut self, offset: u32) -> Query {
        self.offset = Some(offset);
        self
    }

    // Method to build the query string
    pub fn build(self) -> String {
        // Here we would implement the logic to convert the Query struct into a query string
        // For simplicity, this is just a placeholder
        "Generated query string".to_string()
    }
}

impl SupabaseClient {
    pub async fn select(
        &self,
        table_name: &str,
        search_column: &str,
        search_query: &str
    ) -> Result<Vec<Value>, String> {
        let endpoint = format!("{}/rest/v1/{}", self.url, table_name);
        let client = reqwest::Client::new();
        let query = format!("{}=ilike.%25{}%25", search_column, search_query); // Case-insensitive search

        let response = match client
            .get(&endpoint)
            .header("apikey", &self.api_key)
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("Content-Type", "application/json")
            .query(&[("select", "*"), ("filter", &query)])
            .send()
            .await {
                Ok(response) => response,
                Err(e) => return Err(e.to_string())
            };

        if response.status().is_success() {
            let records: Result<Vec<Value>, reqwest::Error> = response.json().await;
            match records {
                Ok(data) => Ok(data),
                Err(e) => Err(e.to_string())
            }
        } else {
            Err(response.status().to_string())
        }
    }
}