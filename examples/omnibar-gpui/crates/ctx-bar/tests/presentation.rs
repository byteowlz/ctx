//! Compile the renderer seam even before the host exports/wires the module.
//! These are pure font/grid tests, not a native focus, IME or screenshot proof.
#[path = "../src/matrix.rs"]
mod matrix;
#[path = "../src/presentation.rs"]
mod presentation;

#[test]
fn public_seams_keep_first_party_input_state() {
    let _: fn(
        ctx_bar_design::BarDesign,
        &gpui_kit::Entity<gpui_kit::component::input::InputState>,
        &mut gpui_kit::Window,
        &mut gpui_kit::App,
    ) -> gpui_kit::AnyElement = presentation::input;
    let _: fn(&mut gpui_kit::App) -> anyhow::Result<()> = presentation::register_font;
}
