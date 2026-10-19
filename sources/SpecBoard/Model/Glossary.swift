import Foundation

/// The words the screen and the MCP share (`specboard://SpecBoard/Docs/Design:Overview` §3): the same as the core's
/// glossary (`sources/core/src/mcp/catalog.json` の `glossary`、MCP の `guide` の
/// `about: glossary`), word for word.
///
/// One list, so a person and an agent reading the same project use the same word for
/// the same thing. The tooltips and `specboard://SpecBoard/Docs/Design:Glossary` are made from it; a word is the
/// key it is written with, so what a person sees is what an agent writes. The lines'
/// words are the names of the kinds of line (`Line`), and the legend on screen is made
/// from the kinds' definitions (`LineKinds`: label, what it means, how it is drawn).
enum Glossary {
    struct Term: Hashable {
        /// As it is written — a key, a kind, a value.
        var word: String
        var meaning: String
    }

    struct Group {
        var title: String
        var terms: [Term]
    }

    /// A row of the legend's table: one kind of line, or the project's own the move makes.
    struct LegendRow: Hashable {
        var words: [String]
        var meaning: String
    }

    static let groups: [Group] = [
        Group(title: "プロジェクトの形", terms: [
            Term(word: "project", meaning: "スキーマと振る舞いの設計図の全体"),
            Term(word: "main", meaning: "いまの実装を表すツリー。プロジェクトに 1 つ"),
            Term(word: "branch", meaning: "変更の計画。main から作り、作ったときの main の全部を見て、branch で書いたものだけが branch のものになる。作ったあとの main の変更は、main を取り込むまで映らない。main からは見えず、branch どうしも見えない"),
            Term(word: "section", meaning: "カードの組。main・branch とボードの根の直下はプロジェクトの project_tree で決まり（種類と名前と並び）、ほかのセクションはその中に作り、入れ子にできる。パスでは Store/Ordering:Order のように書く"),
            Term(word: "project_tree", meaning: "プロジェクトの木。main・branch の根の直下（main_sections）とボードの根の直下（board_sections）のセクションの種類・名前・並び。書き換えると、外したセクションは空なら消え、中身があれば _evacuation_ へ退避する"),
            Term(word: "free", meaning: "フリーセクション。どの種類のカードも置ける下書きの場所。種類の合うセクションへ移して整理する"),
            Term(word: "ui", meaning: "UI のセクション。部品（html・svg のカード）と、ほかの UI のセクションをはめ込むはめ込み（frame）を置いて画面を組む。note も置ける。中に作るセクションも ui。プロジェクトの project_tree に type: ui で足す。画面ではセクションマップで開き、部品をモックと同じ見た目で、置いた所に並び順に重ねて描く。部品の角に見出しの札（headerPlace）と付箋（notePlace）を重ね、図の上のバーの「見出し」「ノート」で出し入れする"),
            Term(word: "mock", meaning: "モック。UI のセクションの部品を、置いた所に置いた大きさで組み上げた 1 枚の HTML のページ（はめ込みには指す先の UI のセクションのモックが入る。note のカードは描かない。スクリプトは動かない）。MCP は section_read の as: mock（html・width・height・parts・frames）で読み、faces: true で部品ごとのページ（faces）も添え、as: picture で絵。画面のセクションマップは部品ごとのページで部品を描く。エクスポートすると UI のセクションごとに 1 枚の .html"),
            Term(word: "board", meaning: "プロジェクトボード。やること（目標と小目標・作業の文書）を置く、main・branch の外の木。merge・ロック・MR は無く、いつでも書ける。MCP は branch: \"_project_board_\"。ボードのカードからは main のカードへ「つながる」線（link）を引け、本文に main のパスを書ける（main からボードへは引けない）"),
            Term(word: "ghost", meaning: "カードゴースト。セクションに置く、ほかのセクションのカード（かカードゴースト）を指すショートカット（macOS のエイリアスのようなもの。コピーではない）。自分の名前を持ち、置いたセクションの中ではその名前だけで書け、ほかからは section:名前 で書ける。指す先（target）は id で覚えて改名・移動に付いて行き、本物が消えると指す先の無いカードゴーストとして残る。セクションの図には指す先のカードが丸ごと出る。カードゴーストの無いほかのセクションのカードは、セクションごとに 1 枚のセクションゴーストにまとまり、指されている所だけが出る。画面で一時的に出すだけのもの（一時カードゴースト）はプロジェクトに残らない"),
            Term(word: "standalone", meaning: "value・enum を自分のセクションの図に、持っているカードの中に畳まずに 1 枚のカードで出す（見かけ。コミットにならない）"),
            Term(word: "base", meaning: "branch で main のカードを初めて書いたときの main の中身。branch で変えたことは base と比べて言う（branch_read）"),
            Term(word: "merge", meaning: "branch で変えたことを main に入れること（作り直しているあいだは使えない）"),
            Term(word: "path", meaning: "section:Card.field の形で場所を指す書き方。書いた場所のツリーの中で探す。区切りの字（空白と / : . [ ] ' @ | { } # \" `）を含む名前は一重引用符で囲む（Docs:'release notes.md'。名前の中の ' は ''）"),
            Term(word: "in", meaning: "入れ物のカード（flow・goal。種類の定義の holds に中に置ける種類がある）の中のカードが入っている入れ物。中のカードのパスは入れ物のパスに名前を続ける（Behavior:Checkout.review・Goals:Launch.Design）。card_update の in で入れ物を移す"),
        ]),
        Group(title: "カードの種類（kind）", terms: [
            Term(word: "entity", meaning: "単位で保存・送信される記録（ID を持つ）"),
            Term(word: "value", meaning: "何かの中に入る形（ID を持たない）。セクションマップでは持っているカードの中に畳んで出る"),
            Term(word: "enum", meaning: "値の集合（open なら網羅でない）。値ごとに表示名と説明を持てる。セクションマップでは持っているカードの中に畳んで出る"),
            Term(word: "flow", meaning: "部品がすることと、部品をまたぐ処理の流れ（MCP のリクエストを受ける・ユーザー登録など。部品はセクションで表す）。入れ物のカードで、中に開始・処理・実行・判断・待つ・合流・終了のノードのカードを置き、線でつなぐ"),
            Term(word: "store", meaning: "記録を置く場所（テーブル・コレクション・台帳・ファイル）"),
            Term(word: "instance", meaning: "データ。entity・value の実例（形を持つ実際の値）。定義のセクション（definition。新しいプロジェクトでは根の Definitions）とフリーセクションに置く。形は線 of（MCP は value のキー of に形のパス）、値は data に持つ。図とエディタは形のフィールドごとに型の札と値を並べる。値 1 つのパスは Data:first.status（並びの要素は Data:first.lines[0]・Data:first.lines[sku=A-1]）。形の JSON Schema の examples と、文書の card の行の表に出る"),
            Term(word: "table", meaning: "表。形（entity・value）のデータ（instance）を決まり（within・where・columns・sort・unnest）で集めた表のカード。定義・ドキュメント・フリーのセクションに置く。形は線 of（MCP は value のキー of に形のパス）。図は組み上がった表の絵で、文書には本文の埋め込みか行 card で表として入り、エクスポートでは Markdown の表"),
            Term(word: "note", meaning: "カードのそばに置く付箋（メモ・変更履歴）。どの種類のセクションにも置け、本文（body）を持つ。セクションマップで本文が見える"),
            Term(word: "freedoc", meaning: "自由文書。ドキュメントセクションに置く Markdown 1 枚（body）の文書。エクスポートすると 1 枚が 1 ファイル"),
            Term(word: "formatdoc", meaning: "整形文書。ドキュメントセクションに置く、項目と行（rows）で組んだ設計書・仕様書。エクスポートすると 1 枚が 1 ファイル"),
            Term(word: "stackdoc", meaning: "積み重ね文書。ドキュメントセクションに置く、版（versions）を新しい順に重ねた文書（変更履歴など）。図とエディタはセレクタで選んだ版を見せる。エクスポートするとカード全体が 1 ファイルで、エクスポートの項目に版のパスを書くとその版だけ"),
            Term(word: "svg", meaning: "SVG の絵。本文（body）に SVG を書き、図の上ではカードの箱に収めて（縦横の比を保って）画像として描く（中のスクリプトは動かず、外のものは読み込まない）。本文は XML として読め、根が svg の要素であること。ドキュメント・フリー・UI のセクションに置き、大きさを変えられる。エクスポートすると 1 枚が 1 ファイル（.svg）"),
            Term(word: "html", meaning: "HTML のページ。本文（body）に HTML（CSS も）を書き、図の上ではカードの箱の幅で組んで箱で切って描く。スクリプト・外の読み込み・フォームの送信は動かない（sb-nav:<パス> のリンクはアプリの中で、http・https はブラウザで開く）。ドキュメント・フリー・UI のセクションに置き、大きさを変えられる。エクスポートすると 1 枚が 1 ファイル（.html。頭に同じ守りを書く）"),
            Term(word: "frame", meaning: "はめ込み。UI のセクションに置き、欄 target の UI のセクションのモックを自分の箱に 1:1 ではめ込む（はみ出す所は切る）。target はパスで書き、id で覚えて改名・移動に付いて行く。自分のいるセクションと、たどると自分のいるセクションに戻るものは指せない。同じセクションを何枚ものはめ込みで使える。大きさを変えられ、既定はカード 1 枚分"),
            Term(word: "goal", meaning: "目標。status（状態）・priority（優先度）・due（期限）・workBranch（作業する branch）・MR の欄と、本文（body）を持つ入れ物のカードで、中に goal と task を置く。中の並びが task の作業の順。プロジェクトボードの goals のセクションとフリーセクションに置く"),
            Term(word: "task", meaning: "タスク。済みのチェック（done）と、本文（body。決めたこと・どこまで進んだか・経緯）と、つながる線（connections の line: link）を持つ。goal の中に入れると、goal の行（番号・チェック・名前）として出る。図の上では種類の定義の face のとおり、チェックと本文の Markdown で描き、大きさを変えられる"),
            Term(word: "file", meaning: "ファイル。作業ディレクトリ（この機械で結び付けたフォルダ）を読んで、コアが作業ディレクトリのセクションの中に作るカード。名前と表示名はファイルの名前（パスでは区切りの字を含む名前を一重引用符で囲む: repo/docs:'CardKinds.md'）、filePath は作業ディレクトリからのパス、language は名前と作業ディレクトリの parsers で決まる言語。読むだけで、main と branch のカードから実装の線（implementedBy）で指す。関数と型を拾う言語のファイルは、中の関数と型（symbol）も指せ、図ではそれを行で並べる"),
            Term(word: "workdir", meaning: "作業ディレクトリのセクション。main と各 branch の根の直下に、プロジェクトの木に種類 workdir で並べたもの。中はこの機械のフォルダから読んで作る読むだけのもので、置けるのはメモだけ。設定はそのセクションの section_update（readType・exclude・description・folder）、読み直すのはアクションの「読み込む」（section_action の op: read）"),
            Term(word: "directory", meaning: "作業ディレクトリのセクションの中のディレクトリ。コアが読んだフォルダから作るセクションで、ファイルが種類 file のカード。読むだけ"),
            Term(word: "symbol", meaning: "関数と型。関数と型を拾う言語の作業ディレクトリのファイルの中の関数・メソッド・型・モジュール・定数・マクロで、コアが同梱の tree-sitter の文法で読むたびにファイルから作るカード（保存しない）。パスはファイルのパスに続ける（repo/src:'a.rs'.Order.total。同じ親に同じ名前が並べば、あとのものに「 (2)」）。of（どの言語でも同じ分け方。画面の札は ƒ が関数とメソッド・T が型・M がモジュール・C が定数・# がマクロ）・word（その言語での呼び方: fn・struct・protocol など）・language を持つ。main と branch のカードから実装の線（implementedBy）で指せ、関数と型から線は出せない。card_read で宣言の 1 行（declaration）・中身（body）・呼んでいる名前（calls）を読む"),
            Term(word: "parsers", meaning: "言語の対応。作業ディレクトリのファイルの拡張子（.h）か名前（Makefile）から、読む言語の id（cpp。null で言語なし）への対応。作業ディレクトリのセクションの section_update の parsers で既定を書き換え、parsers: null で既定に戻す。読み方（readType・exclude）と同じくプロジェクトの設定で、section_read に効いている対応と言語（languages）が出る"),
        ]),
        Group(title: "flow のノード", terms: [
            Term(word: "begin", meaning: "開始。trigger と next の線。受けて始まる（receive）ときは相手のフローへの peer の線（何本でも）と program（どちらも省くと呼んだ側）・carries"),
            Term(word: "trigger", meaning: "開始のきっかけ: manual（手動）・receive（受ける）・event（ほかのフローの終わり）・schedule（定期）・startup（起動のたび）"),
            Term(word: "process", meaning: "処理。steps にまっすぐ続く行（読む・書く・署名・確かめる・呼ぶ・送る・受ける・計算する）と next の線。行が読む・書く所は reads・writes の線（from に行）"),
            Term(word: "run", meaning: "実行。runs の線のフローを呼んで終わりを待つ（carries を渡し、returns を受ける）。onError・next の線"),
            Term(word: "eval", meaning: "判断。value と、branch の線（欄 when）・else の線"),
            Term(word: "wait", meaning: "待つ。for（approval / timer / event）・timeout と、onTimeout・next の線"),
            Term(word: "join", meaning: "合流。next の線を 2 本以上にして分かれた行き先がそろうのを待つ（join: all / any / first、partial: fail / continue）"),
            Term(word: "end", meaning: "終了。outcome（success / failure / cancelled）がフローの戻り値。返すものは return"),
            Term(word: "diagram", meaning: "flow の図の形の既定: flowchart（コンテンツマップの図）/ sequence（シーケンス図）。エクスポート・文書の図・MCP の図はこの形で描く"),
            Term(word: "topLeft", meaning: "カードやノードの左上の位置（マス。{x, y}）"),
        ]),
        Group(title: "note と文書", terms: [
            Term(word: "body", meaning: "note と freedoc の本文（Markdown）。[[パス]] で欄を指し（[[セクション:]] でセクション、[[パス|字]] で見せる字を変える）、{{パス}} で値（const のある欄・列挙・データの値）を差し込み、![[パス]] の 1 行で図を入れる"),
            Term(word: "rows", meaning: "formatdoc の行。name（パスに使う名前）と kind: item（項目。heading と中の rows、5 段まで）・text（本文）・table（表）・diagram（図）・diagrams（セクションの図）・card（カードの表）。パスはフィールドと同じ形（Docs:Spec.auth.login）"),
            Term(word: "versions", meaning: "stackdoc の版。name（パスに使う名前）・key（セレクタに出す名前）・heading（見出し。省くと key）・body（本文の Markdown）。並びは新しい版が上で、版のパスは Docs:Changelog.v0_2_21"),
            Term(word: "version", meaning: "stackdoc で開いている版（版の name。見かけで、コミットにならない）。無ければ一番上の版"),
        ]),
        Group(title: "フィールド", terms: [
            Term(word: "required", meaning: "キーが必ずある"),
            Term(word: "nullable", meaning: "null が正当な値（required とは独立）"),
            Term(word: "const", meaning: "常にこの値を取る"),
            Term(word: "data", meaning: "データ（instance）の値。形と同じ入れ子の JSON（キーはフィールドの名前、並びの欄は配列）。書いた値は形のフィールドの型・nullable・const・列挙の値・判別子の枝と、文字の値は欄の rules の pattern で確かめ、合わなければ断る。required の欠けは書き途中として点検に出る。形から消えたフィールドの値は strayFields に残る"),
        ]),
        Group(title: "線（connections）", terms: [
            Term(word: "connections", meaning: "カードの線の並び。1 本ずつ line（線の種類）・from（出る欄や行。省けば頭）・to（相手）・at（相手の中の着く所）・description・fields（線の種類が決める欄）"),
            Term(word: "line", meaning: "線の種類。組み込み（embed・points・link・holds・of・notes・mentions と、flow のノードの線 next・branch・else・onError・onTimeout・runs・event・reads・writes・peer・key）とプロジェクトが定義するもの。意味（定義の base）は embed（型が相手になる）・designate（型は変わらず相手を指す）・link（型に関係しない）。kind_list・kind_read で読み、kind_create・kind_update で定義する"),
            Term(word: "embed", meaning: "内包: 欄が相手の形をそのまま持つ（生成される型が変わる。欄の type は ref と読む）"),
            Term(word: "points", meaning: "ID で指す: 欄の値が相手の ID（型は変わらない）"),
            Term(word: "of", meaning: "形: データ（instance）と表（table）から形（entity・value）への線。1 本だけで、MCP は value のキー of に形のパスを書く（connections には書かない）。図にははじめから描かず、カードの頭に形の名前が出る"),
            Term(word: "at", meaning: "線が相手の中で着く所（欄か行。相手の外なら、そこのフルパス）"),
            Term(word: "fromBoard", meaning: "関わるやること。main（と branch）のカードを線か本文のパスで指している、ボードのカードのうち済んでいないもの（済んだ task、完了・打ち切りの goal とその中のカードは出さない）。プロパティパネルと card_read に出る"),
        ]),
        Group(title: "処理ノードの行（steps）", terms: [
            Term(word: "receive", meaning: "受ける（相手のフローへの peer の線は何本でも引け、program と一緒にも書ける。どちらも省くと、呼んだ側から）"),
            Term(word: "return", meaning: "終了ノードが返すもの（carries。返す相手は peer の線（何本でも）と program で、どちらも省くと呼んだ側へ）"),
            Term(word: "call", meaning: "プロジェクトに無い道具（program）を呼んで返りを待つ（carries を送り、returns を受ける）。プロジェクトの flow を呼んで待つのは実行ノード"),
            Term(word: "send", meaning: "渡す（返りを待たない。ほかの flow を始めさせるのも）。相手は peer の線（何本でも。相手ごとに渡す）か program で、どちらかが要る"),
            Term(word: "read", meaning: "置き場所から読む（reads の線）"),
            Term(word: "write", meaning: "置き場所へ書く（writes の線）"),
            Term(word: "sign", meaning: "署名する（key の線、use: sign）"),
            Term(word: "verify", meaning: "署名を確かめる（key の線、use: verify）"),
            Term(word: "compute", meaning: "計算。業務の検証や計算のような、外に出ない処理（何をするかは行の description。ほかの欄・線は持たない）"),
            Term(word: "via", meaning: "受け渡しの方法: stdio / file / exec / http / grpc / websocket / queue と、プロジェクトが足した値（kind_update の flow の vias。省くと部品の中）"),
            Term(word: "initiator", meaning: "呼びに行く側: self / peer（データの向きとは別）"),
        ]),
    ]

    /// The legend's title, what it says first, and its rows.
    static let legendTitle = "セクションマップの線（凡例）"
    static let legendNote = "線の種類の名前と、画面の名前（定義の label）。凡例は線の種類の定義（label・説明・look）から出す。"
    static let legend: [LegendRow] = [
        LegendRow(words: ["embed"], meaning: "内包: 欄が相手の形をそのまま持つ（欄の型が相手になる）"),
        LegendRow(words: ["points"], meaning: "ID で指す: 欄の値が相手の ID（型は変わらない）"),
        LegendRow(words: ["link"], meaning: "つながる: 型に関係しない、つながりだけの線（task からほかの task へ、など。ボードのカードから main のカードへも）"),
        LegendRow(words: ["holds"], meaning: "置く: store が置く記録の形"),
        LegendRow(words: ["of"], meaning: "形: データ（instance）の形と、表（table）が集めるデータの形（entity・value）。はじめは描かない（カードの頭に形の名前が出る）"),
        LegendRow(words: ["notes"], meaning: "決まり: flow の決まりを書いた note・freedoc・formatdoc か、formatdoc の項目"),
        LegendRow(words: ["implementedBy"], meaning: "実装: 仕様・設計のカードから、それを実装している作業ディレクトリのファイルか、その中の関数と型へ（main と branch から。相手は木の中のパス repo/src:'a.rs'・repo/src:'a.rs'.Order.total）"),
        LegendRow(words: ["mentions"], meaning: "言及: note と文書の本文の [[ ]] が指す所（本文から拾う）"),
        LegendRow(words: ["next"], meaning: "次: flow のノードの次の行き先（2 本以上で並列に分かれる）"),
        LegendRow(words: ["branch"], meaning: "条件: 判断（eval）の分かれ道。条件（欄 when）が当たるときの行き先"),
        LegendRow(words: ["else"], meaning: "それ以外: 判断（eval）のどの条件にも当たらないときの行き先"),
        LegendRow(words: ["onError"], meaning: "失敗したら: 実行（run）が失敗したときの行き先"),
        LegendRow(words: ["onTimeout"], meaning: "期限切れ: 待つ（wait）が期限を過ぎたときの行き先"),
        LegendRow(words: ["runs"], meaning: "実行する: 実行（run）が呼んで終わりを待つフロー"),
        LegendRow(words: ["event"], meaning: "待つ終わり: 開始・待つが待つ、ほかのフローの終わり（欄 outcome にその結果）"),
        LegendRow(words: ["reads"], meaning: "読む: 処理の行（read）が読む所"),
        LegendRow(words: ["writes"], meaning: "書く: 処理の行（write）が書く所"),
        LegendRow(words: ["peer"], meaning: "相手: 送る・受ける・返す相手のフロー（1 つの所から何本でも。同じフローへは 1 本）"),
        LegendRow(words: ["key"], meaning: "鍵を使う: 署名する（use: sign）・確かめる（use: verify）鍵"),
        LegendRow(words: ["digest", "signedBy", "verifiedBy"], meaning: "プロジェクトの線（前の形のプロジェクトを移したときに作る）: ハッシュで指す・署名を作る鍵・署名を確かめる鍵"),
    ]

    static func term(_ word: String) -> Term? {
        groups.lazy.flatMap(\.terms).first { $0.word == word }
    }

    /// The glossary as a document, for a person (the project's `specboard://SpecBoard/Docs/Design:Glossary`): the core's
    /// glossary, as MCP's `guide` gives it. The ways a body names a place (`[[パス]]` and the like), written in a
    /// meaning, are put in code: the document is a card of the project, and there they would be read as paths.
    static var markdown: String {
        var out = "# 用語集\n\n画面と MCP で同じ語を使う。語はそれを書くときのキーや値そのもの。"
            + "この文書はテスト `GlossaryTests` が `sources/SpecBoard/Model/Glossary.swift` から書き出し、プロジェクトの Docs/Design:Glossary に置く。\n"
        for group in groups {
            out += "\n## \(group.title)\n\n| 語 | 意味 |\n|---|---|\n"
            for term in group.terms { out += "| `\(term.word)` | \(cell(term.meaning)) |\n" }
        }
        out += "\n## \(legendTitle)\n\n\(legendNote)\n\n| 語 | 意味 |\n|---|---|\n"
        for row in legend {
            out += "| \(row.words.map { "`\($0)`" }.joined(separator: "・")) | \(cell(row.meaning)) |\n"
        }
        return out
    }

    /// A meaning as a table cell: a `|` in it would end the cell, even inside code.
    private static func cell(_ text: String) -> String {
        asCode(text).replacingOccurrences(of: "|", with: "\\|")
    }

    /// `[[…]]`, `![[…]]` and `{{…}}` in words, put in code: each where the note scanner closes it
    /// (`NoteText.bracketed`).
    private static func asCode(_ text: String) -> String {
        var out = ""
        var at = text.startIndex
        for span in NoteText.bracketed(text) {
            out += text[at..<span.lowerBound] + "`" + text[span] + "`"
            at = span.upperBound
        }
        return out + text[at...]
    }
}
