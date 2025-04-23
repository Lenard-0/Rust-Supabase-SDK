#[cfg(test)]
mod tests {
    use rust_supabase_sdk::select::{Filter, FilterGroup, LogicalOperator, Operator, SelectQuery, Sort, SortDirection};

    #[test]
    fn test_filter_to_query() {
        let filter = Filter::new("name", Operator::Eq, "Test Organisation");
        assert_eq!(filter.to_query(), "name=eq.Test%20Organisation");
    }

    #[test]
    fn test_filter_group_or_to_query() {
        let filter1 = Filter::new("name", Operator::Eq, "Test Organisation");
        let filter2 = Filter::new("id", Operator::Eq, "123");
        let group = FilterGroup::new(LogicalOperator::Or, vec![filter1, filter2]);
        assert_eq!(group.to_query_string(), "or=(name.eq.Test%20Organisation,id.eq.123)");
    }

    #[test]
    fn test_filter_group_and_to_query() {
        let filter1 = Filter::new("name", Operator::Eq, "Test Organisation");
        let filter2 = Filter::new("id", Operator::Eq, "123");
        let group = FilterGroup::new(LogicalOperator::And, vec![filter1, filter2]);
        assert_eq!(group.to_query_string(), "name=eq.Test%20Organisation&id=eq.123");
    }

    #[test]
    fn test_sort_to_query() {
        let sort = Sort::new("created_at", SortDirection::Desc);
        assert_eq!(sort.to_query(), "order=created_at.desc");
    }

    #[test]
    fn test_select_query_to_query_string_with_filter_group() {
        // Build a filter group with an AND operator containing two filters.
        let filter_group = FilterGroup::new(
            LogicalOperator::And,
            vec![
                Filter::new("name", Operator::Eq, "Test Organisation"),
                Filter::new("id", Operator::Eq, "123"),
            ],
        );
        let sort = Sort::new("created_at", SortDirection::Asc);

        let mut query = SelectQuery::new();
        query.filter = Some(filter_group);
        query.sorts.push(sort);

        // Expected:
        // select=%2A&name=eq.Test%20Organisation&id=eq.123&order=created_at.asc
        let expected = "select=%2A&name=eq.Test%20Organisation&id=eq.123&order=created_at.asc";
        assert_eq!(query.to_query_string(), expected);
    }
}