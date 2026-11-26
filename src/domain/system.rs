//! The operating system multigit runs on, where a default differs by it.

/// An operating system with defaults of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Linux,
    MacOs,
    Windows,
}

impl System {
    /// The system this build is for; every other Unix counts as Linux.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_build_knows_its_system() {
        let expected = if cfg!(target_os = "macos") {
            System::MacOs
        } else if cfg!(windows) {
            System::Windows
        } else {
            System::Linux
        };
        assert_eq!(System::current(), expected);
    }
}
