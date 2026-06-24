use reqwest::StatusCode;

use crate::helpers::spawn_app;

#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    // Arrange
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    //Act
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let response = client.post(format!("{}/subscriptions",app.address))
    .header("Content-Type", "application/x-www-form-urlencoded")
    .body(body)
    .send()
    .await
    .expect("Failed to execute request");

    //Assert
    assert_eq!(StatusCode::OK,response.status());

    let saved = sqlx::query!("SELECT email,name FROM subscriptions")
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email,"ursula_le_guin@gmail.com");
    assert_eq!(saved.name,"le guin")

}

#[tokio::test]
async fn subscribe_returns_a_422_for_invalid_form_data() {
    //Arrange
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    //Act
    let body = "name=le%20guin&mail=ursula_le_guin%40gmail.com";

    let response = client.post(format!("{}/subscriptions",app.address)).header("Content-Type", "application/x-www-form-urlencoded")
    .body(body)
    .send()
    .await
    .expect("Failed to execute request");

    //Assert
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY,response.status())
}