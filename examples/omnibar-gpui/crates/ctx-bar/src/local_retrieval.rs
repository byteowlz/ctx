//! Optional independent local transport: coalesced, stale-safe and off the UI thread.
use std::time::Duration;

use gpui_kit::Context;

use ctx_bar::config::descriptor_path;
use ctx_bar::proxy::Proxy;

use super::Bar;

impl Bar {
    pub(super) fn schedule_local(&mut self, cx: &mut Context<Self>) {
        if self.review
            || !self.catalog_ready
            || !self.catalog.retrieval_enabled
            || self.state.remote_request().is_none()
            || self.local_generation == Some(self.state.generation)
        {
            return;
        }
        let generation = self.state.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(35))
                .await;
            let work = this
                .update(cx, |this, _| {
                    if generation != this.state.generation
                        || this.local_busy
                        || this.local_generation == Some(generation)
                    {
                        return None;
                    }
                    let request = this.state.remote_request()?;
                    this.local_busy = true;
                    this.local_generation = Some(generation);
                    Some((request, this.catalog.clone()))
                })
                .ok()
                .flatten();
            let Some((request, catalog)) = work else {
                return;
            };
            let task = cx.background_executor().spawn(async move {
                descriptor_path()
                    .and_then(|path| Proxy::new(&path, 14)?.retrieve(&request, &catalog))
                    .map_err(|_| "Local retrieval unavailable".to_string())
            });
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.local_busy = false;
                if this.state.complete_local(generation, result) {
                    this.scroll.scroll_to_item(0);
                } else if generation != this.state.generation {
                    this.schedule_local(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn browse_all(&mut self, cx: &mut Context<Self>) {
        if !self.catalog_ready || self.local_choices.is_some() {
            return;
        }
        if let Ok(items) = self.catalog.candidates(&self.state.request()) {
            let items = items.into_iter().cloned().collect();
            if self.state.browse(items) {
                self.scroll.scroll_to_item(0);
                cx.notify();
            }
        }
    }
}
