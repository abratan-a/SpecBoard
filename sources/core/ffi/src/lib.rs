//! コアを Swift から呼ぶ束ね（UniFFI）。窓口の 4 つ（開く・呼ぶ・知らせを受ける・閉じる）と、`app_control` に
//! 答え図の絵を描く口を渡す `set_app` をそのまま出す。

use std::sync::Arc;

uniffi::setup_scaffolding!();

/// 開けなかったわけ。Swift では投げられるエラーになる。
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum CoreError {
    Open(String),
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::Open(message) => write!(f, "{message}"),
        }
    }
}

/// 知らせを受けるもの。Swift の側で作って `Core::listen` に渡す。
#[uniffi::export(with_foreign)]
pub trait CoreListener: Send + Sync {
    fn changed(&self, notice: String);
}

struct Relay(Arc<dyn CoreListener>);

impl specboard_core::Listener for Relay {
    fn changed(&self, notice: &str) {
        self.0.changed(notice.to_string());
    }
}

/// `app_control`（画面を読む・動かす）に答え、人に許可を求め（`ask`）、図の絵を描く（`picture`）もの。Swift の側で
/// 作って `Core::set_app` に渡す。引数と答えの形は `specboard_core::App`。呼ばれるのは MCP の要求を受けたスレッド
/// （`Core::call` を呼んだスレッド）。
#[uniffi::export(with_foreign)]
pub trait CoreApp: Send + Sync {
    fn app_control(&self, arguments: String, caller: String) -> String;
    fn ask(&self, request: String) -> String;
    fn picture(&self, request: String) -> String;
}

struct AppRelay(Arc<dyn CoreApp>);

impl specboard_core::App for AppRelay {
    fn app_control(&self, arguments: &str, caller: &str) -> String {
        self.0
            .app_control(arguments.to_string(), caller.to_string())
    }

    fn ask(&self, request: &str) -> String {
        self.0.ask(request.to_string())
    }

    fn picture(&self, request: &str) -> String {
        self.0.picture(request.to_string())
    }
}

/// 開いたコア。
#[derive(uniffi::Object)]
pub struct Core {
    core: specboard_core::Core,
}

#[uniffi::export]
impl Core {
    /// データベースのファイル（コアの形）を開く。
    #[uniffi::constructor]
    pub fn open(place: String) -> Result<Arc<Self>, CoreError> {
        specboard_core::Core::open(place)
            .map(|core| Arc::new(Core { core }))
            .map_err(|e| CoreError::Open(e.to_string()))
    }

    /// 新しい空のデータベース（コアの形）を作って開く。もうあれば断る。
    #[uniffi::constructor]
    pub fn create(place: String) -> Result<Arc<Self>, CoreError> {
        specboard_core::Core::create(place)
            .map(|core| Arc::new(Core { core }))
            .map_err(|e| CoreError::Open(e.to_string()))
    }

    /// 頼みの JSON を受けて、答えの JSON を返す。
    pub fn call(&self, request: String) -> String {
        self.core.call(&request)
    }

    /// 知らせを受けるものを足す。
    pub fn listen(&self, listener: Arc<dyn CoreListener>) {
        self.core.listen(Arc::new(Relay(listener)));
    }

    /// `app_control` に答え、人に許可を求め、図の絵を描くものを渡す（あとから渡したものが答える）。
    pub fn set_app(&self, app: Arc<dyn CoreApp>) {
        self.core.set_app(Arc::new(AppRelay(app)));
    }

    /// 閉じる。
    pub fn close(&self) {
        self.core.close();
    }
}
