//! GPU display only. Authoritative document samples will live in fotoeditor-core.
#[cfg(target_arch = "wasm32")]
mod compositor;
#[cfg(target_arch = "wasm32")]
mod filter;
#[cfg(target_arch = "wasm32")]
pub use filter::FilterTileInput;
#[cfg(target_arch = "wasm32")]
mod preview;
#[cfg(target_arch = "wasm32")]
mod renderer;
#[cfg(target_arch = "wasm32")]
pub use renderer::Renderer;
#[cfg(target_arch = "wasm32")]
mod selection;
#[cfg(target_arch = "wasm32")]
mod viewport;

#[cfg(test)]
mod tests {
    #[test]
    fn filter_shader_is_valid_portable_wgsl() {
        let m = naga::front::wgsl::parse_str(include_str!("filter.wgsl")).expect("filter WGSL");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&m)
        .expect("portable filter shader");
    }
    #[test]
    fn preview_shader_is_valid_portable_wgsl() {
        let module =
            naga::front::wgsl::parse_str(include_str!("preview.wgsl")).expect("preview WGSL");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("portable preview shader");
    }
    #[test]
    fn selection_shader_is_valid_portable_wgsl() {
        let module =
            naga::front::wgsl::parse_str(include_str!("selection.wgsl")).expect("selection WGSL");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("portable selection shader");
    }
    #[test]
    fn composite_shader_is_valid_portable_wgsl() {
        let module =
            naga::front::wgsl::parse_str(include_str!("composite.wgsl")).expect("composite WGSL");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("compositor must validate without optional GPU capabilities");
    }
    #[test]
    fn diagnostic_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(include_str!("diagnostic.wgsl"))
            .expect("diagnostic shader must parse");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("diagnostic shader must validate without optional GPU capabilities");
    }
    #[test]
    fn viewport_shader_is_valid_wgsl() {
        let module =
            naga::front::wgsl::parse_str(include_str!("viewport.wgsl")).expect("viewport WGSL");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("portable viewport shader");
    }
}
