//! The render path the hardware allows (SYS-040: software adapts, never limited by one
//! hardware): a GPU path where there is one that does what is asked (shaders for plots and
//! effects, textures large enough), software otherwise, each feature falling back on its own,
//! visibly (reduced effects, design system §12.4), never silently broken.

/// What the hardware offers, as the system detects it (the device description and the driver).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// OpenGL ES major version (0: none).
    pub gles: u8,
    pub max_texture: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Path {
    Gpu,
    Software,
}

/// What a feature needs to run on the GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Needs {
    pub gles: u8,
    pub texture: u32,
}

/// The path for a feature: the GPU when the hardware meets its needs, software otherwise.
pub fn path(caps: Capabilities, needs: Needs) -> Path {
    if caps.gles >= needs.gles && caps.gles > 0 && caps.max_texture >= needs.texture { Path::Gpu } else { Path::Software }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gpu_where_it_can_software_otherwise() {
        let t1 = Capabilities { gles: 3, max_texture: 8192 };
        let virtual_board = Capabilities { gles: 0, max_texture: 0 };
        let spectrum = Needs { gles: 3, texture: 2048 };
        assert_eq!(path(t1, spectrum), Path::Gpu);
        assert_eq!(path(virtual_board, spectrum), Path::Software);
        assert_eq!(path(Capabilities { gles: 2, max_texture: 4096 }, spectrum), Path::Software);
    }
}
