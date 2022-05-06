use std::fmt;

use futures::channel::mpsc;
use futures::stream::SplitSink;
use futures::{Sink, SinkExt, StreamExt};
use gloo_console::log;
use gloo_net::websocket;
use gloo_net::websocket::futures::WebSocket;
use sycamore::futures::spawn_local_scoped;
use sycamore::prelude::*;
use web_sys::Event;

#[derive(Prop, Clone, PartialEq, Eq)]
struct Message {
    // TODO: can this be a reference?
    body: String,
    color: Color,

    // TODO: should this field be named pull?
    direction: Pull,
}

fn main() {
    log!("starting");

    sycamore::render(|cx| {
        // TODO: how to newtype this?
        let history = create_signal(cx, Vec::new());

        websocket(cx, history);

        view! { cx,
            div(class="container is-max-desktop is-flex is-flex-direction-column", style="height: 100%") {
                div(class="container is-flex is-flex-direction-column-reverse", style="width: 100%; overflow:auto") {
                    List(history)
                }
                Input(history)
            }
        }
    });
}

fn websocket<'a>(cx: Scope<'a>, history: &'a Signal<Vec<Message>>) {
    log!("connecting to websocket");
    let ws = match WebSocket::open("ws://192.168.1.29:3000/ws") {
        Ok(ws) => {
            log!("connected to websocket");
            ws
        }
        Err(e) => {
            log!("failed to connect to websocket: ", e.to_string());
            panic!("FUUUUCK");
        }
    };

    let (write, mut read) = ws.split();
    let (tx, rx) = mpsc::unbounded();

    spawn_local_scoped(cx, async move {
        log!("spawning reader");

        while let Some(next) = read.next().await {
            if let Ok(websocket::Message::Text(text)) = next {
                history.modify().push(Message {
                    body: text,
                    color: Color::Info,
                    direction: Pull::Left,
                });
            }
        }
    });

    spawn_local_scoped(cx, async move {
        log!("spawning writer");

        let res = rx
            .map(|m| Ok(websocket::Message::Text(m)))
            .forward(write)
            .await;

        if let Err(e) = res {
            log!("error in writer:", e.to_string());
        }
    });

    let mut last_index = 0;
    create_effect(cx, move || {
        let history = history.get();

        history.iter().skip(last_index).for_each(|m| {
            if matches!(m.direction, Pull::Right) {
                tx.unbounded_send(m.body.clone()).unwrap();
            }
        });
        last_index = history.len();
    });
}

#[component]
fn Input<'a, G: Html>(cx: Scope<'a>, history: &'a Signal<Vec<Message>>) -> View<G> {
    let new_msg = create_signal(cx, String::new());

    let submit = |e: Event| {
        // The default behavior of the submit event refreshes the page.
        e.prevent_default();

        // Taking the value out of the input field clears it, since it's bound below.
        let trimmed = new_msg.take().trim().to_string();

        if !trimmed.is_empty() {
            history.modify().push(Message {
                body: trimmed,
                color: Color::Empty,
                direction: Pull::Right,
            });
        }
    };

    view! { cx,
        form(class="field", on:submit=submit) {
            input(class="input", type="text", bind:value=new_msg)
        }
    }
}

#[component]
fn List<'a, G: Html>(cx: Scope<'a>, history: &'a ReadSignal<Vec<Message>>) -> View<G> {
    view! { cx,
        // The `p-3` padding override is necessary to work around an apparent bug in bulma columns.
        // Without it, there is a slight horizontal overflow and scroll.
        ul(class="p-3") {
            // TODO: the iteration code is pretty beefy, and checks a bunch of stuff we don't care
            // about (our old messages never change). Is there a way to not use it that would
            // actually be more lightweight?
            Indexed {
                iterable: history,
                view: |cx, x| view! { cx,
                    li(class="columns is-mobile") {
                        Bubble(x)
                    }
                }
            }
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Pull {
    Left,
    Right,
}

impl fmt::Display for Pull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pull = match self {
            Pull::Left => "is-pulled-left",
            Pull::Right => "is-pulled-right",
        };

        write!(f, "{pull}")
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Color {
    Empty,
    Primary,
    Link,
    Info,
    Success,
    Warning,
    Danger,
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let class = match self {
            Color::Empty => "",
            Color::Primary => "is-primary",
            Color::Link => "is-link",
            Color::Info => "is-info",
            Color::Success => "is-success",
            Color::Warning => "is-warning",
            Color::Danger => "is-danger",
        };

        write!(f, "{class}")
    }
}

#[component]
fn Bubble<G: Html>(cx: Scope, props: Message) -> View<G> {
    let position = match props.direction {
        Pull::Left => "is-11",
        Pull::Right => "is-11 is-offset-1",
    };

    let class = format!("notification {} {}", props.color, props.direction);

    view! { cx,
        div(class=format!("column {position}")) {
            div(class=(class)) {
                p { (props.body) }
            }
        }
    }
}
