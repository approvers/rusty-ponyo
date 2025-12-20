use {
    anyhow::Context as _,
    axum::{
        Json, Router,
        extract::{Path, Query, State},
        http::StatusCode,
        response::{IntoResponse, Response},
        routing::get,
    },
    rusty_ponyo::{
        bot::meigen::{
            FindOptions, MeigenDatabase, SortDirection, SortKey,
            model::{Meigen, MeigenId},
        },
        db,
    },
    serde::Deserialize,
    serde_json::json,
    std::net::SocketAddr,
    tokio::net::TcpListener,
    tower_http::trace::TraceLayer,
    tracing::error,
};

assert_one_feature!("mongo_db", "memory_db");

#[cfg(feature = "mongo_db")]
type Db = db::mongodb::MongoDb;

#[cfg(feature = "memory_db")]
type Db = db::mem::MemoryDB;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let addr: SocketAddr = std::env::var("INTERNAL_API_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse()
        .context("failed to parse INTERNAL_API_ADDR")?;

    let db = build_db().await?;

    let app = Router::new()
        .route("/meigen/:id", get(get_meigen_by_id))
        .route("/meigen", get(search_meigen))
        .route("/meigen/count", get(count_meigen))
        .layer(TraceLayer::new_for_http())
        .with_state(db);

    tracing_subscriber::fmt()
        .with_ansi(std::env::var("NO_COLOR").is_err())
        .init();

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(feature = "mongo_db")]
async fn build_db() -> anyhow::Result<Db> {
    let uri = env_var("MONGODB_ATLAS_URI")?;
    Db::new(&uri).await
}

#[cfg(feature = "memory_db")]
async fn build_db() -> anyhow::Result<Db> {
    Ok(Db::new())
}

#[derive(Deserialize)]
struct SearchQuery {
    author: Option<String>,
    content: Option<String>,
    offset: Option<u32>,
    limit: Option<u8>,
    sort: Option<SortKey>,
    dir: Option<SortDirection>,
    #[serde(default)]
    random: bool,
}

async fn get_meigen_by_id(
    State(db): State<Db>,
    Path(id): Path<MeigenId>,
) -> Result<Json<Meigen>, ApiError> {
    match db.load(id).await {
        Ok(Some(meigen)) => Ok(Json(meigen)),
        Ok(None) => Err(ApiError::NotFound(format!("meigen No.{id} not found"))),
        Err(err) => Err(ApiError::Internal(err)),
    }
}

async fn search_meigen(
    State(db): State<Db>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<Meigen>>, ApiError> {
    let sort = q.sort.unwrap_or_default();
    let dir = q.dir.unwrap_or_default();
    let offset = q.offset.unwrap_or(0);
    let limit = q.limit.unwrap_or(5);

    if !(1..=10).contains(&limit) {
        return Err(ApiError::BadRequest(
            "limit must be between 1 and 10".into(),
        ));
    }

    let options = FindOptions {
        author: q.author.as_deref(),
        content: q.content.as_deref(),
        offset,
        limit,
        sort,
        dir,
        random: q.random,
    };

    db.search(options).await.map(Json).map_err(Into::into)
}

async fn count_meigen(State(db): State<Db>) -> Result<Json<CountResponse>, ApiError> {
    db.count()
        .await
        .map(|count| Json(CountResponse { count }))
        .map_err(ApiError::from)
}

#[derive(serde::Serialize)]
struct CountResponse {
    count: u32,
}

#[derive(Debug)]
enum ApiError {
    BadRequest(String),
    NotFound(String),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for ApiError {
    fn from(value: anyhow::Error) -> Self {
        Self::Internal(value)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
            }
            ApiError::NotFound(msg) => {
                (StatusCode::NOT_FOUND, Json(json!({ "error": msg }))).into_response()
            }
            ApiError::Internal(err) => {
                error!(?err, "internal error while handling request");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "internal server error" })),
                )
                    .into_response()
            }
        }
    }
}

fn env_var(name: &str) -> anyhow::Result<String> {
    std::env::var(name).with_context(|| format!("failed to get {name} environment variable"))
}

macro_rules! assert_one_feature {
    ($a:literal, $b: literal) => {
        #[cfg(all(feature = $a, feature = $b))]
        compile_error!(concat!(
            "You can't enable both of ",
            $a,
            " and ",
            $b,
            " feature at the same time."
        ));

        #[cfg(not(any(feature = $a, feature = $b)))]
        compile_error!(concat!(
            "You must enable either ",
            $a,
            " or ",
            $b,
            " feature."
        ));
    };
}

use assert_one_feature;
