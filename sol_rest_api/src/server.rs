use actix_web::dev::Server;
use actix_web::{middleware, web, App, HttpServer};
use tracing::info;

use crate::routes::{get_account, get_transaction, index};
use sol_db::DbPool;

/// Http server to serve the REST API of the aggregator
pub fn http_server(db_pool: DbPool, host: String, port: u16) -> anyhow::Result<Server> {
    info!("Starting HTTP server at http://{}:{}", host, port);

    Ok(HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(db_pool.clone()))
            .wrap(middleware::Logger::default())
            .service(web::resource("/").to(index))
            .service(get_transaction)
            .service(get_account)
    })
    .bind((host, port))?
    .run())
}

#[cfg(test)]
mod tests {
    use actix_web::{http::StatusCode, test};
    use sol_db::initialize_db_pool;
    use super::*;

    #[actix_web::test]
    async fn account_get() {

        let pool = initialize_db_pool("./testu.db".to_string());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .wrap(middleware::Logger::default())
                .service(get_transaction)
            // .service(add_user),
        ).await;

        let req = test::TestRequest::get().uri("/accounts42/?id=123").to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND); // Error 404: transactions42 endpoints does not exist
        
        let req = test::TestRequest::get().uri("/accounts/?foo=123").to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND); // Error 404: wrong url param
        
        let req = test::TestRequest::get().uri("/accounts/?id=123").to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND); // Error 404: cannot find this account

        let req = test::TestRequest::get().uri("/accounts/").to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND); // Error 404: cannot find any accounts
    }
}
