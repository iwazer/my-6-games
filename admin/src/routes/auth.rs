use axum::{
    extract::{Query, Request, State},
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use openidconnect::{
    core::CoreResponseType, reqwest::async_http_client, AuthenticationFlow, AuthorizationCode,
    CsrfToken, Nonce, Scope, TokenResponse,
};
use serde::Deserialize;
use tera::Context;

use crate::{middleware::extract_session_id, session, state::AppState};

/// ログインページ（エラー表示用）を Tera でレンダリング
fn render_login(state: &AppState, error: Option<&str>) -> Response {
    let mut ctx = Context::new();
    if let Some(msg) = error {
        ctx.insert("error", msg);
    }
    match state.tera.render("login.html.tera", &ctx) {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("テンプレートレンダリングエラー: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /auth/login — Auth0 の認可エンドポイントへリダイレクト
pub async fn login(State(state): State<AppState>) -> impl IntoResponse {
    let (auth_url, csrf_token, nonce) = state
        .oidc_client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .url();

    let mut redis = state.redis.clone();
    if let Err(e) =
        session::store_oauth_state(&mut redis, csrf_token.secret(), nonce.secret()).await
    {
        tracing::error!("OAuth state の保存に失敗: {}", e);
        return render_login(
            &state,
            Some("内部エラーが発生しました。再度お試しください。"),
        );
    }

    Redirect::to(auth_url.as_str()).into_response()
}

#[derive(Deserialize)]
pub struct CallbackParams {
    code: String,
    state: String,
}

/// GET /auth/callback — Auth0 からのコールバック処理
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Response {
    let mut redis = state.redis.clone();

    // CSRF state を検証して nonce を取得（GETDEL で使い捨て）
    let nonce_secret = match session::pop_oauth_nonce(&mut redis, &params.state).await {
        Ok(Some(n)) => n,
        Ok(None) => {
            tracing::warn!("無効な OAuth state: {}", params.state);
            return render_login(
                &state,
                Some("認証セッションが無効です。再度ログインしてください。"),
            );
        }
        Err(e) => {
            tracing::error!("nonce の取得に失敗: {}", e);
            return render_login(
                &state,
                Some("内部エラーが発生しました。再度お試しください。"),
            );
        }
    };

    // Authorization Code を Token に交換
    let token_response = match state
        .oidc_client
        .exchange_code(AuthorizationCode::new(params.code))
        .request_async(async_http_client)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("トークン交換に失敗: {}", e);
            return render_login(&state, Some("認証に失敗しました。再度お試しください。"));
        }
    };

    // ID token を検証してクレームを取得
    let nonce = Nonce::new(nonce_secret);
    let id_token = match token_response.id_token() {
        Some(t) => t,
        None => {
            tracing::error!("ID token が存在しません");
            return render_login(&state, Some("認証に失敗しました。再度お試しください。"));
        }
    };
    let claims = match id_token.claims(&state.oidc_client.id_token_verifier(), &nonce) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("ID token の検証に失敗: {}", e);
            return render_login(&state, Some("認証に失敗しました。再度お試しください。"));
        }
    };

    // email を取得してホワイトリスト確認
    let email = match claims.email() {
        Some(e) => e.to_string(),
        None => {
            return render_login(&state, Some("メールアドレスが取得できませんでした。"));
        }
    };
    if !state.config.access.allowed_emails.contains(&email) {
        tracing::warn!("アクセス拒否: {}", email);
        return render_login(
            &state,
            Some(&format!("{} はアクセスが許可されていません。", email)),
        );
    }

    // セッションを作成して Cookie を発行
    let session_id =
        match session::create_session(&mut redis, &email, state.config.session.ttl_seconds).await {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("セッション作成に失敗: {}", e);
                return render_login(
                    &state,
                    Some("内部エラーが発生しました。再度お試しください。"),
                );
            }
        };

    tracing::info!("ログイン成功: {}", email);

    let cookie = format!(
        "{}={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}",
        crate::middleware::SESSION_COOKIE,
        session_id,
        state.config.session.ttl_seconds
    );
    let mut response = Redirect::to("/").into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    response
}

/// GET /auth/logout — セッション削除 + Auth0 ログアウト
pub async fn logout(State(state): State<AppState>, request: Request) -> impl IntoResponse {
    if let Some(session_id) = extract_session_id(request.headers()) {
        let mut redis = state.redis.clone();
        if let Err(e) = session::delete_session(&mut redis, &session_id).await {
            tracing::warn!("セッション削除に失敗: {}", e);
        }
    }

    let return_to =
        urlencoding::encode(&format!("{}/auth/login", state.config.server.base_url)).into_owned();
    let logout_url = format!(
        "https://{}/v2/logout?client_id={}&returnTo={}",
        state.config.auth0.domain, state.config.auth0.client_id, return_to,
    );

    let clear_cookie = format!(
        "{}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
        crate::middleware::SESSION_COOKIE
    );
    let mut response = Redirect::to(&logout_url).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_cookie).unwrap(),
    );
    response
}
