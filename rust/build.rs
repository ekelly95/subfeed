use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("dev.subfeed.core"))
        .qt_module("Qml")
        .files(["src/app_info.rs", "src/background.rs", "src/feed_model.rs", "src/player_session.rs", "src/setup.rs", "src/ytdlp_status.rs"])
        .build();
}
