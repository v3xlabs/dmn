use yew::prelude::*;
use yew::ServerRenderer;

#[function_component]
fn App() -> Html {
    html! {<div>{"Hello, World!"}</div>}
}

#[poem::handler]
pub async fn web_endpoint() -> poem_openapi::payload::Html<String> {
    let html = ServerRenderer::<App>::new().render().await;
    poem_openapi::payload::Html(html)
}
