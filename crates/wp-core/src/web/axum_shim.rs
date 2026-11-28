//! [웹판] axum 의 `tokio` 기능(소켓)에만 있는 ConnectInfo 를 같은 모양으로.
//! 브라우저에는 상대 주소가 없으므로, 요청에 넣어 준 값(Extension)이 있으면 그것을 돌려준다.
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;

#[derive(Clone, Copy, Debug)]
pub struct ConnectInfo<T>(pub T);

impl<S, T> FromRequestParts<S> for ConnectInfo<T>
where
    S: Send + Sync,
    T: Clone + Send + Sync + 'static,
{
    type Rejection = StatusCode;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<ConnectInfo<T>>().cloned().ok_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
}
