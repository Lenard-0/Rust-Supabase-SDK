
#[cfg(test)]
mod tests {
    use dotenv::dotenv;
    use rust_supabase_sdk::{auth::SignUpRequest, SupabaseClient};

    #[tokio::test]
    async fn can_create_user_and_sign_in() {
        dotenv().ok();
        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_SERVICE_WORKER").unwrap(),
            None
        );

        let email = "test_user_system1234@fakemail.com";
        let password = "password123";
        let auth_response = supabase_client.sign_up(SignUpRequest {
            email: email.to_string(),
            password: password.to_string(),
            user_id: None,
            name: Some("Test User".to_string())
        }).await.unwrap();
        println!("Auth response: {:#?}", auth_response.user);
        assert_eq!(auth_response.user["email"], email);
        assert_eq!(auth_response.user["name"], "Test User");
        assert_eq!(auth_response.token_type, "bearer");

        let sign_in = supabase_client.sign_in(email, password).await.unwrap();
        assert_eq!(sign_in.user["email"], email);
        assert_eq!(sign_in.token_type, "bearer");

        let user_info = supabase_client.get_user(&sign_in.access_token).await.unwrap();
        assert_eq!(user_info["email"], email);

        // Clean up
        // println!("Deleting user: {}", user_info);
        // supabase_client.delete("auth.users", &user_info["id"].as_str().unwrap()).await.unwrap();
    }
}