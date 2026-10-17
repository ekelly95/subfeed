//! `AppInfo`: a QML singleton proving the Rust core is wired into the UI.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, version)]
        #[qproperty(QString, core)]
        type AppInfo = super::AppInfoRust;
    }
}

use cxx_qt_lib::QString;

pub struct AppInfoRust {
    version: QString,
    core: QString,
}

impl Default for AppInfoRust {
    fn default() -> Self {
        Self {
            version: QString::from(env!("CARGO_PKG_VERSION")),
            core: QString::from("Rust core online"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_cargo() {
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.2");
    }
}
