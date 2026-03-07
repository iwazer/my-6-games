use rocket::Request;
use rocket_dyn_templates::{context, Template};

#[catch(404)]
pub fn not_found(_req: &Request<'_>) -> Template {
    Template::render(
        "error",
        context! {
            code: 404,
            reason: "ページが見つかりません",
            description: "お探しのページは存在しないか、有効期限が切れています",
        },
    )
}

#[catch(422)]
pub fn unprocessable(_req: &Request<'_>) -> Template {
    Template::render(
        "error",
        context! {
            code: 422,
            reason: "リクエストが正しくありません",
            description: "入力内容を確認してください",
        },
    )
}

#[catch(500)]
pub fn internal_error(_req: &Request<'_>) -> Template {
    Template::render(
        "error",
        context! {
            code: 500,
            reason: "サーバーエラー",
            description: "しばらく経ってからもう一度お試しください",
        },
    )
}
