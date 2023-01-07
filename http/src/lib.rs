use blake_tree::StreamStorage;
use tide::Request;

pub async fn server(store: StreamStorage) -> tide::Server<StreamStorage> {
    let mut app = tide::with_state(store);
    app.at("/").get(list);
    app.at("/").post(add);
    app.at("/:id").get(read);
    app.at("/:id").delete(remove);
    app.at("/:id/ranges").post(ranges);
    app.at("/:id/missing_ranges").post(missing_ranges);
    app
}

async fn list(_req: Request<StreamStorage>) -> tide::Result {
    todo!()
}

async fn add(_req: Request<StreamStorage>) -> tide::Result {
    todo!()
}

async fn read(_req: Request<StreamStorage>) -> tide::Result {
    todo!()
}

async fn ranges(_req: Request<StreamStorage>) -> tide::Result {
    todo!()
}

async fn missing_ranges(_req: Request<StreamStorage>) -> tide::Result {
    todo!()
}

async fn remove(_req: Request<StreamStorage>) -> tide::Result {
    todo!()
}
