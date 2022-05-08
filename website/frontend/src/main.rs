use std::fmt;

use futures::channel::mpsc;
use futures::StreamExt;
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
            div(class="max-w-md mx-auto flex flex-col h-full") {
                div(class="flex flex-col-reverse w-full h-full overflow-auto") {
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
                    color: Color::Blue,
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
                color: Color::Grey,
                direction: Pull::Right,
            });
        }
    };

    view! { cx,
        form(class="field", on:submit=submit) {
            input(class="w-full rounded", type="text", bind:value=new_msg)
        }
    }
}

#[component]
fn List<'a, G: Html>(cx: Scope<'a>, history: &'a ReadSignal<Vec<Message>>) -> View<G> {
    view! { cx,
        ul(class="p-3 grid grid-cols-12 gap-4") {
            // TODO: the iteration code is pretty beefy, and checks a bunch of stuff we don't care
            // about (our old messages never change). Is there a way to not use it that would
            // actually be more lightweight?
            Indexed {
                iterable: history,
                view: |cx, x| view! { cx,
                    Bubble(x)
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
            Pull::Left => "justify-self-start",
            Pull::Right => "justify-self-end",
        };

        write!(f, "{pull}")
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Color {
    Grey,
    Blue,
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let class = match self {
            Color::Grey => "bg-slate-300",
            //Color::Primary => "bg-teal-400",
            //Color::Link => "bg-blue-600",
            Color::Blue => "bg-blue-500 text-white",
            //Color::Success => "bg-green-400",
            //Color::Warning => "bg-yellow-300",
            //Color::Danger => "bg-red-500",
        };

        write!(f, "{class}")
    }
}

#[component]
fn Bubble<G: Html>(cx: Scope, props: Message) -> View<G> {
    let position = match props.direction {
        Pull::Left => "col-span-11",
        Pull::Right => "col-start-2 col-span-11",
    };

    let class = format!(
        "rounded-lg p-2 {} {} {}",
        position, props.color, props.direction
    );

    view! { cx,
        li(class=(class)) {
            p { (props.body) }
        }
    }
}
