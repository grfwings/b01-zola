use axum::{
    Form, Router,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode},
    response::Html,
    routing::get,
};
use serde::Deserialize;
use sqlx::sqlite::SqlitePool;
use std::{str::FromStr, sync::Arc};
use tower_http::cors::{AllowHeaders, CorsLayer};

struct AppState {
    db: SqlitePool,
    userinfo_url: String,
}

#[derive(Deserialize)]
struct UserInfo {
    sub: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

struct AuthUser {
    user_id: String,
    name: String,
    email: String,
}

#[derive(Deserialize)]
struct CreateCommentForm {
    content: String,
    #[serde(default)]
    website: Option<String>,
}

#[derive(sqlx::FromRow)]
struct Comment {
    author_name: String,
    author_website: Option<String>,
    content: String,
    created_at: String,
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn render_comment(comment: &Comment) -> String {
    let author = escape_html(&comment.author_name);
    let content = escape_html(&comment.content);
    let date = escape_html(&comment.created_at);

    let author_html = match &comment.author_website {
        Some(url) if url.starts_with("https://") || url.starts_with("http://") => {
            let escaped = escape_html(url);
            format!(r#"<a href="{escaped}" rel="nofollow noopener" target="_blank">{author}</a>"#)
        }
        _ => author.to_string(),
    };

    format!(
        r#"<div class="comment">
  <div class="comment-header">
    <strong>{author_html}</strong>
    <span class="comment-date">{date}</span>
  </div>
  <div class="comment-body">{content}</div>
</div>"#
    )
}

async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<AuthUser, StatusCode> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?
        .to_string();

    let url = state.userinfo_url.clone();

    let info = tokio::task::spawn_blocking(move || -> Result<UserInfo, StatusCode> {
        let mut resp = ureq::get(&url)
            .header("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(|e| {
                eprintln!("Userinfo request failed: {e}");
                match e {
                    ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => {
                        StatusCode::UNAUTHORIZED
                    }
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                }
            })?;

        resp.body_mut().read_json::<UserInfo>().map_err(|e| {
            eprintln!("Failed to parse userinfo response: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
    })
    .await
    .map_err(|e| {
        eprintln!("Auth task panicked: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })??;

    Ok(AuthUser {
        user_id: info.sub,
        name: info.name.unwrap_or_else(|| "Anonymous".to_string()),
        email: info.email.unwrap_or_default(),
    })
}

async fn fetch_comments_html(db: &SqlitePool, post_slug: &str) -> Result<Html<String>, StatusCode> {
    let comments: Vec<Comment> = sqlx::query_as(
        "SELECT author_name, author_website, content, created_at \
         FROM comments WHERE post_slug = ? ORDER BY created_at ASC",
    )
    .bind(post_slug)
    .fetch_all(db)
    .await
    .map_err(|e| {
        eprintln!("Failed to fetch comments: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if comments.is_empty() {
        return Ok(Html(
            r#"<p class="no-comments">No comments yet. Be the first!</p>"#.to_string(),
        ));
    }

    let html: String = comments
        .iter()
        .map(render_comment)
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Html(html))
}

async fn get_comments(
    State(state): State<Arc<AppState>>,
    Path(post_slug): Path<String>,
) -> Result<Html<String>, StatusCode> {
    fetch_comments_html(&state.db, &post_slug).await
}

async fn create_comment(
    State(state): State<Arc<AppState>>,
    Path(post_slug): Path<String>,
    headers: HeaderMap,
    Form(form): Form<CreateCommentForm>,
) -> Result<Html<String>, StatusCode> {
    let user = authenticate(&state, &headers).await?;

    let content = form.content.trim().to_string();
    if content.is_empty() || content.len() > 10_000 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let website = form
        .website
        .filter(|w| !w.is_empty())
        .filter(|w| w.starts_with("https://") || w.starts_with("http://"));

    sqlx::query(
        "INSERT INTO comments (post_slug, author_name, author_email, author_website, content, user_id) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&post_slug)
    .bind(&user.name)
    .bind(&user.email)
    .bind(&website)
    .bind(&content)
    .bind(&user.user_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        eprintln!("Failed to insert comment: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    fetch_comments_html(&state.db, &post_slug).await
}

#[tokio::main]
async fn main() {
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:comments.db".to_string());

    let userinfo_url = std::env::var("USERINFO_URL")
        .unwrap_or_else(|_| "https://b01-idm.com/auth/v1/oidc/userinfo".to_string());

    let allowed_origin = std::env::var("CORS_ORIGIN")
        .unwrap_or_else(|_| "https://www.betweenzeroand.one".to_string());

    let db_opts = sqlx::sqlite::SqliteConnectOptions::from_str(&database_url)
        .expect("Invalid DATABASE_URL")
        .create_if_missing(true);

    let db = SqlitePool::connect_with(db_opts)
        .await
        .expect("Failed to connect to database");

    sqlx::migrate!()
        .run(&db)
        .await
        .expect("Failed to run migrations");

    let state = Arc::new(AppState {
        db,
        userinfo_url,
    });

    let cors = CorsLayer::new()
        .allow_origin(
            allowed_origin
                .parse::<axum::http::HeaderValue>()
                .expect("Invalid CORS_ORIGIN"),
        )
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(AllowHeaders::mirror_request());

    let app = Router::new()
        .route(
            "/api/comments/{post_slug}",
            get(get_comments).post(create_comment),
        )
        .with_state(state)
        .layer(cors);

    let addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:3001".to_string());

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");

    println!("Listening on {addr}");
    axum::serve(listener, app).await.expect("Server error");
}
