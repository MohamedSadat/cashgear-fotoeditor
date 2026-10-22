//! GPU display only. Authoritative document samples will live in fotoeditor-core.
#[cfg(target_arch = "wasm32")]
mod renderer;
#[cfg(target_arch = "wasm32")]
pub use renderer::Renderer;

#[cfg(test)]
mod tests {
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
}
