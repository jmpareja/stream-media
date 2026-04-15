use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, Response};
use common::error::AppError;

pub async fn proxy_request(
    client: &reqwest::Client,
    backend_base_url: &str,
    req: Request<Body>,
    strip_prefix: &str,
) -> Result<Response<Body>, AppError> {
    let uri = req.uri();
    let path = uri.path();
    let remaining_path = path.strip_prefix(strip_prefix).unwrap_or(path);
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    let backend_url = format!("{backend_base_url}{remaining_path}{query}");

    let method = req.method().clone();
    let headers = req.headers().clone();

    // Collect the request body
    let body_bytes = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024 * 1024) // 10GB limit
        .await
        .map_err(|e| AppError::Internal(format!("failed to read request body: {e}")))?;

    let mut backend_req = client.request(method, &backend_url);

    // Forward relevant headers
    for (name, value) in headers.iter() {
        let name_str = name.as_str();
        match name_str {
            "host" | "connection" | "transfer-encoding" => continue,
            _ => {
                if let Ok(val) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
                    backend_req = backend_req.header(name_str, val);
                }
            }
        }
    }

    if !body_bytes.is_empty() {
        backend_req = backend_req.body(body_bytes);
    }

    let backend_resp = backend_req
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("backend request failed: {e}")))?;

    let status = backend_resp.status();
    let resp_headers = backend_resp.headers().clone();

    let resp_bytes = backend_resp
        .bytes()
        .await
        .map_err(|e| AppError::Internal(format!("failed to read backend response: {e}")))?;

    let mut response = Response::new(Body::from(resp_bytes));
    *response.status_mut() = axum::http::StatusCode::from_u16(status.as_u16())
        .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);

    for (name, value) in resp_headers.iter() {
        if let Ok(val) = HeaderValue::from_bytes(value.as_bytes()) {
            response.headers_mut().insert(name.clone(), val);
        }
    }

    Ok(response)
}
