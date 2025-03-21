
#[cfg(test)]
mod tests {
    use dotenv::dotenv;
    use rust_supabase_sdk::{auth::{AuthErrorResponse, SignUpRequest}, SupabaseClient};

    #[tokio::test]
    async fn can_create_user_and_sign_in() {
        dotenv().ok();
        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_SERVICE_WORKER").unwrap(),
            None
        );

        let email = "test_user_system1234567@fakemail.com";
        let password = "password123";
        let auth_response = supabase_client.sign_up(SignUpRequest {
            email: email.to_string(),
            password: password.to_string(),
            user_id: None,
            name: Some("Test User".to_string())
        }).await.unwrap();
        println!("Auth response: {:#?}", auth_response.user);
        assert_eq!(auth_response.user["email"], email);
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

    #[tokio::test]
    async fn can_throw_correct_error_creating_user_that_already_exists() {
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
        }).await;

        match auth_response {
            Ok(_) => panic!("User was created successfully"),
            Err(auth_error_response) => {
                assert_eq!(auth_error_response, AuthErrorResponse {
                    code: 422,
                    msg: "User already registered".to_string(),
                    error_code: "user_already_exists".to_string()
                });
            }
        }
    }

    #[tokio::test]
    async fn can_do_forget_password() {
        dotenv().ok();
        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_SERVICE_WORKER").unwrap(),
            None
        );

        let email = "telyqujo@dreamclarify.org";
        supabase_client.forgot_password(email).await.unwrap();
    }

    #[tokio::test]
    async fn can_do_reset_password() {
        dotenv().ok();
        let supabase_client = SupabaseClient::new(
            std::env::var("SUPABASE_URL").unwrap(),
            std::env::var("SUPABASE_SERVICE_WORKER").unwrap(),
            None
        );

        let new_password = "password123";
        let code = "798759";
        let access_token = "eyJhbGciOiJIUzI1NiIsImtpZCI6IkN6VVk1VHBCYitJMXF2b1IiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL3V5aHVkbW50eXF3ZGlibWZiZ3FxLnN1cGFiYXNlLmNvL2F1dGgvdjEiLCJzdWIiOiJhYmQyNzBiZC1kMjc2LTQ4ZDYtOWY4Yi1jZjJiMDM1M2Y4NzMiLCJhdWQiOiJhdXRoZW50aWNhdGVkIiwiZXhwIjoxNzQyNTQxOTIwLCJpYXQiOjE3NDI1MzgzMjAsImVtYWlsIjoidGVseXF1am9AZHJlYW1jbGFyaWZ5Lm9yZyIsInBob25lIjoiIiwiYXBwX21ldGFkYXRhIjp7InByb3ZpZGVyIjoiZW1haWwiLCJwcm92aWRlcnMiOlsiZW1haWwiXX0sInVzZXJfbWV0YWRhdGEiOnsiZW1haWwiOiJ0ZWx5cXVqb0BkcmVhbWNsYXJpZnkub3JnIiwiZW1haWxfdmVyaWZpZWQiOnRydWUsImZpcnN0X25hbWUiOiJmYWtlIiwiZnVsbF9uYW1lIjoiZmFrZSBmYWtlIiwibGFzdF9uYW1lIjoiZmFrZSIsInBob25lX3ZlcmlmaWVkIjpmYWxzZSwic3ViIjoiYWJkMjcwYmQtZDI3Ni00OGQ2LTlmOGItY2YyYjAzNTNmODczIn0sInJvbGUiOiJhdXRoZW50aWNhdGVkIiwiYWFsIjoiYWFsMSIsImFtciI6W3sibWV0aG9kIjoib3RwIiwidGltZXN0YW1wIjoxNzQyNTM4MzIwfV0sInNlc3Npb25faWQiOiIyZWNiYTlmNi1jZGVhLTQyNmItODViYS00MmZkMTgwYjVlNWMiLCJpc19hbm9ueW1vdXMiOmZhbHNlfQ.07cbSN7lQMwVaHDZ3Hpg_yQ-UeWoVGGxtcizkt0mngE";
        supabase_client.reset_password(new_password, access_token, code).await.unwrap();
    }
}