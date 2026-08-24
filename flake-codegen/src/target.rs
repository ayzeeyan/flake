//! Target architecture and operating system definitions.

use std::fmt;
use std::str::FromStr;

use crate::error::CodegenError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetArch {
    X86_64,
    Aarch64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetOs {
    Windows,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target {
    pub arch: TargetArch,
    pub os: TargetOs,
}

impl Target {
    pub const X86_64_WINDOWS: Self = Self {
        arch: TargetArch::X86_64,
        os: TargetOs::Windows,
    };

    pub const X86_64_LINUX: Self = Self {
        arch: TargetArch::X86_64,
        os: TargetOs::Linux,
    };

    pub const AARCH64_LINUX: Self = Self {
        arch: TargetArch::Aarch64,
        os: TargetOs::Linux,
    };

    #[must_use]
    pub fn host() -> Self {
        #[cfg(target_os = "linux")]
        {
            #[cfg(target_arch = "aarch64")]
            {
                Self::AARCH64_LINUX
            }
            #[cfg(not(target_arch = "aarch64"))]
            {
                Self::X86_64_LINUX
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self::X86_64_WINDOWS
        }
    }

    #[must_use]
    pub fn is_windows(&self) -> bool {
        self.os == TargetOs::Windows
    }

    #[must_use]
    pub fn is_linux(&self) -> bool {
        self.os == TargetOs::Linux
    }

    #[must_use]
    pub fn is_x86_64(&self) -> bool {
        self.arch == TargetArch::X86_64
    }

    #[must_use]
    pub fn is_aarch64(&self) -> bool {
        self.arch == TargetArch::Aarch64
    }

    #[must_use]
    pub fn default_extension(&self) -> &'static str {
        match self.os {
            TargetOs::Windows => "exe",
            TargetOs::Linux => "",
        }
    }
}

impl Default for Target {
    fn default() -> Self {
        Self::host()
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.arch, self.os) {
            (TargetArch::X86_64, TargetOs::Windows) => write!(f, "x86_64-windows"),
            (TargetArch::X86_64, TargetOs::Linux) => write!(f, "x86_64-linux"),
            (TargetArch::Aarch64, TargetOs::Linux) => write!(f, "aarch64-linux"),
            (TargetArch::Aarch64, TargetOs::Windows) => write!(f, "aarch64-windows"),
        }
    }
}

impl FromStr for Target {
    type Err = CodegenError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let norm = s.to_ascii_lowercase().replace('_', "-");
        match norm.as_str() {
            "x86-64-windows"
            | "x86-64-windows-pe"
            | "x86-64-pc-windows-msvc"
            | "x86-64-w64-mingw32"
            | "x86-64-windows-gnu"
            | "windows"
            | "win"
            | "win64"
            | "windows-x86-64"
            | "x86-64-pe" => Ok(Self::X86_64_WINDOWS),

            "x86-64-linux"
            | "x86-64-linux-elf"
            | "x86-64-unknown-linux-gnu"
            | "x86-64-unknown-linux-musl"
            | "linux"
            | "linux64"
            | "linux-x86-64"
            | "x86-64-elf" => Ok(Self::X86_64_LINUX),

            "aarch64-linux"
            | "aarch64-linux-elf"
            | "aarch64-unknown-linux-gnu"
            | "aarch64-unknown-linux-musl"
            | "arm64-linux"
            | "arm64"
            | "aarch64" => Ok(Self::AARCH64_LINUX),

            other => Err(CodegenError::new(format!(
                "unsupported target triple '{other}'. Supported targets: x86_64-windows, x86_64-linux, aarch64-linux"
            ))),
        }
    }
}
