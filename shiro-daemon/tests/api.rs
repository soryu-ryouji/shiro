// 集成测试：路由与鉴权中间件（不触盘：/health 无鉴权，startup 只返回内存状态）。
// 端点行为与数据目录相关的用例由 tools/smoke-api.sh 覆盖（临时 HOME 隔离）。

use axum::body::Body;
use axum::http::{Request, StatusCode};
use shiro_daemon::app::build_router;
use shiro_daemon::state::AppState;
use tower::ServiceExt;

const TOKEN: &str = "test-token";

fn router() -> axum::Router {
    let state = AppState {
        token: TOKEN.into(),
        watch_hub: Default::default(),
        chat: Default::default(),
    };
    build_router(state, None).0
}

#[tokio::test]
async fn health_requires_no_auth() {
    let res = router()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn protected_route_rejects_missing_or_wrong_token() {
    for auth in [None, Some("Bearer wrong")] {
        let mut req = Request::builder().method("POST").uri("/api/v1/app/startup");
        if let Some(a) = auth {
            req = req.header("Authorization", a);
        }
        let res = router()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn startup_returns_ready_with_token() {
    let res = router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/app/startup")
                .header("Authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("\"ready\""));
}

#[tokio::test]
async fn unknown_route_is_404() {
    let res = router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/nope")
                .header("Authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
