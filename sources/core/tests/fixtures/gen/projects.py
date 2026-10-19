#!/usr/bin/env python3
"""The fixture projects the tests start from, made up from nothing real.

Writes, from the definitions below:

- `projects/large.json`: a project of many cards (a fictional city library network), for what only
  shows at size — long lists read page by page, a value object embedded in many places, deep
  nesting, lines of a kind of the project's own, lines that pick their target by a discriminator.
- `projects/before/large.json`: the same project in the project file of version 18, when each card
  held its own export settings (`exportTo`), for the tests that read a file of before.
- `projects/datamodel.json`: a small data model (a library's lending) of four enums and five
  entities, nested objects, a field that refers to its own card and a history-like entity.
- `projects/small.json`: one card of nested objects.

and the copies the app's tests read (`sources/SpecBoardTests/Fixtures/`). The output is the same on
every run: ids are derived from names, times are fixed.

    python3 sources/core/tests/fixtures/gen/projects.py          # write the files
    python3 sources/core/tests/fixtures/gen/projects.py --check  # say whether they are up to date

After writing, retake the kept answers (`SPECBOARD_KEEP=1 cargo test --workspace` in sources/core)
and look at their diff.
"""

import hashlib
import json
import sys
import uuid
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURES = HERE.parent
REPO = FIXTURES.parents[3]
SWIFT_FIXTURES = REPO / "sources" / "SpecBoardTests" / "Fixtures"

VERSION = 28
VERSION_BEFORE = 18
CARD_V = 7


def uid(*parts):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, "specboard-fixture:" + "/".join(parts)))


def line_id(*parts):
    return hashlib.sha1(("specboard-fixture-line:" + "/".join(parts)).encode()).hexdigest()[:24]


# ---------------------------------------------------------------------------------------------
# The definitions' shorthand.


def f(
    name,
    type=None,
    *,
    d=None,
    label=None,
    opt=False,
    arr=False,
    null=False,
    unique=False,
    const=None,
    pk=False,
    rules=None,
    variants=None,
    emb=None,
    pts=None,
    key=None,
    sub=None,
):
    """A field. `emb` embeds a card; `pts` (target, cardinality) points at one by its id; `key`
    (target, cardinality) refers to one by a key of the project's own line kind; `sub` is the
    fields of an object defined in place."""
    return {
        "name": name,
        "type": "object" if sub is not None and type is None else type,
        "d": d,
        "label": label,
        "req": not opt,
        "arr": arr,
        "null": null,
        "unique": unique,
        "const": const,
        "pk": pk,
        "rules": rules,
        "variants": variants,
        "emb": emb,
        "pts": pts,
        "key": key,
        "sub": sub,
    }


def enum(name, label, d, values):
    return {"kind": "enum", "name": name, "label": label, "d": d, "values": values}


def value(name, label, d, fields, **extra):
    return {"kind": "value", "name": name, "label": label, "d": d, "fields": fields, **extra}


def entity(name, label, d, fields, **extra):
    return {"kind": "entity", "name": name, "label": label, "d": d, "fields": fields, **extra}


def rec(name):
    """The two fields every record of the network starts with: what it is, and its form's version."""
    kebab = "".join("-" + c.lower() if c.isupper() else c for c in name).lstrip("-")
    return [
        f(
            "record_type",
            d="記録の種類の判別子。どの記録も同じ名前の欄に固定の値を持つので、受け取った側はこの欄だけで形を選べる",
            const=f"lib.{kebab}",
        ),
        f("schema_version", "int", const=1),
    ]


# ---------------------------------------------------------------------------------------------
# The large project: a fictional city library network.

LARGE_ENUMS = [
    enum("CurrencyCode", "通貨", "金額の単位。館の会計は円で締めるが、海外の出版社への発注と寄贈の評価額は外貨で受ける。換算は会計の側で行い、ここでは持たない。",
         [("JPY", "円"), ("USD", "米ドル"), ("EUR", "ユーロ")]),
    enum("RoundingMode", "端数の扱い", "延滞金と按分した送料に 1 円未満が出たときの扱い。",
         [("floor", "切り捨て"), ("round", "四捨五入"), ("ceil", "切り上げ")]),
    enum("MaterialType", "資料の種類", "貸出の規則と棚の割り当てを分ける単位。電子書籍は棚を持たないが、貸出の規則は同じ表で引く。",
         [("book", "図書"), ("magazine", "雑誌"), ("audio", "音声"), ("video", "映像"), ("map", "地図"),
          ("kit", "学習キット"), ("ebook", "電子書籍"), ("score", "楽譜"), ("newspaper", "新聞"), ("game", "ボードゲーム")]),
    enum("MemberCategory", "利用者の区分", "貸出の上限と延滞金の率を変える区分。年齢で自動では変えず、更新の窓口で確かめて変える。",
         [("adult", "一般"), ("child", "子ども"), ("student", "学生"), ("senior", "高齢者"), ("corporate", "団体")]),
    enum("MemberStatus", "利用者の状態", None, [("active", None), ("suspended", None), ("expired", None)]),
    enum("LoanStatus", "貸出の状態", "返却の記録が付いた時点で returned。紛失の届けが出たものは lost にして延滞金を止める。",
         [("open", "貸出中"), ("returned", "返却済み"), ("overdue", "延滞"), ("lost", "紛失")]),
    enum("ReservationStatus", "予約の状態", None,
         [("waiting", "順番待ち"), ("ready", "取り置き中"), ("collected", "受け取り済み"), ("cancelled", "取り消し"), ("expired", "期限切れ")]),
    enum("TransferStatus", "配送の状態", "館間配送の荷の状態。荷を開けて数が合わないときは rejected にして送り元へ戻す。",
         [("requested", None), ("packed", None), ("in_transit", None), ("received", None), ("rejected", None)]),
    enum("CopyCondition", "資料の傷み", None, [("new", "新品"), ("good", "良"), ("worn", "傷みあり"), ("damaged", "破損")]),
    enum("ShelfZone", "棚の区域", "開架・参考・児童・閉架。閉架の資料は予約を通してだけ出す。",
         [("general", "一般開架"), ("reference", "参考"), ("children", "児童"), ("closed_stack", "閉架")]),
    enum("Weekday", "曜日", None, [("mon", None), ("tue", None), ("wed", None), ("thu", None), ("fri", None), ("sat", None), ("sun", None)]),
    enum("NotificationChannel", "知らせる手段", "利用者が選んだ手段で知らせる。郵便は費用がかかるので、延滞の三度目の知らせだけに使う。",
         [("email", "メール"), ("sms", "ショートメッセージ"), ("post", "郵便"), ("app", "アプリ")]),
    enum("FineReason", "延滞金の理由", None, [("overdue", "延滞"), ("damage", "汚損"), ("loss", "紛失")]),
    enum("PaymentMethod", "払い方", None, [("cash", None), ("card", None), ("transfer", None), ("waiver", "免除")]),
    enum("AcquisitionMethod", "受け入れの方法", None, [("purchase", "購入"), ("donation", "寄贈"), ("exchange", "交換")]),
    enum("OrderStatus", "発注の状態", "一部だけ届いた発注は partially_received のまま、残りが届くか取り消すまで閉じない。",
         [("draft", None), ("ordered", None), ("partially_received", None), ("received", None), ("cancelled", None)]),
    enum("VendorKind", "取引先の種類", None, [("publisher", "出版社"), ("distributor", "取次"), ("bookstore", "書店")]),
    enum("EventKind", "行事の種類", None, [("reading", "読み聞かせ"), ("workshop", "工作"), ("exhibition", "展示"), ("lecture", "講演")]),
    enum("AudienceAge", "対象の年齢", None, [("children", None), ("teens", None), ("adults", None), ("all", None)]),
    enum("RoomKind", "部屋の種類", None, [("study", "学習室"), ("meeting", "会議室"), ("hall", "ホール")]),
    enum("BookingStatus", "予約の確定", None, [("held", None), ("confirmed", None), ("cancelled", None)]),
    enum("StaffRole", "職員の役割", "記録に名前を残す職員の役割。ボランティアは承認の欄に名前を書けない。",
         [("librarian", "司書"), ("clerk", "事務"), ("manager", "館長"), ("volunteer", "ボランティア")]),
    enum("ShiftKind", "勤務の区分", None, [("morning", None), ("afternoon", None), ("evening", None)]),
    enum("AuditAction", "監査の操作", None, [("create", None), ("update", None), ("delete", None), ("approve", None), ("reject", None)]),
    enum("RecordKind", "記録の種類", "監査と索引が指す記録の種類。record_type の値から接頭辞を外したもの。",
         [("loan-record", None), ("reservation", None), ("transfer-manifest", None), ("purchase-order", None), ("fine-assessment", None)]),
    enum("LanguageCode", "言語", None, [("ja", "日本語"), ("en", "英語"), ("zh", "中国語"), ("ko", "韓国語"), ("es", "スペイン語")]),
    enum("ClassificationScheme", "分類法", None, [("ndc", "日本十進分類法"), ("ddc", "デューイ十進分類法"), ("local", "館の独自分類")]),
    enum("ItemFormat", "形態", None, [("print", None), ("large_print", "大活字"), ("braille", "点字"), ("digital", None)]),
    enum("DeliveryMode", "配送の手段", None, [("van", "巡回車"), ("courier", "宅配"), ("post", "郵便")]),
    enum("VanRoute", "巡回車の経路", "巡回車は週に二回、四つの経路で分館を回る。経路の外の館へは宅配で送る。",
         [("north", None), ("south", None), ("east", None), ("west", None)]),
    enum("HoldPickup", "受け取りの場所", None, [("branch", "館の窓口"), ("locker", "受け取りロッカー"), ("home_delivery", "宅配")]),
    enum("RenewalRule", "延長の可否", None, [("allowed", None), ("once", None), ("never", None)]),
    enum("SeriesStatus", "シリーズの状態", None, [("ongoing", None), ("completed", None), ("discontinued", None)]),
    enum("DigitalLicense", "電子資料の契約", None, [("one_user", "同時 1 人"), ("multi_user", "同時複数"), ("unlimited", "無制限")]),
    enum("AccessLevel", "公開の範囲", None, [("public", None), ("staff", None), ("restricted", None)]),
    enum("ApprovalDecision", "承認の結果", None, [("approved", "承認"), ("rejected", "却下"), ("deferred", "保留")]),
    enum("SurveyScale", "満足度", "利用者アンケートの五段階。3 を「ふつう」とする。",
         [("1", None), ("2", None), ("3", None), ("4", None), ("5", None)]),
    enum("WeedingReason", "除籍の理由", None, [("outdated", "内容が古い"), ("damaged", "破損"), ("duplicate", "重複"), ("low_use", "利用が少ない")]),
    enum("BudgetPeriod", "予算の期間", None, [("monthly", None), ("quarterly", None), ("yearly", None)]),
    enum("ContactPurpose", "連絡先の用途", None, [("billing", "請求"), ("notices", "お知らせ"), ("emergency", "緊急")]),
    enum("DocumentKind", "本人確認の書類", None, [("id_card", None), ("passport", None), ("student_card", None), ("health_card", None)]),
    enum("CatalogKind", "索引の種類", "索引の一行が何の索引かを言う。種類ごとに指す相手が違う。",
         [("subject", "件名"), ("author", "著者"), ("series", "シリーズ")]),
    enum("TemperatureZone", "保管の温度帯", None, [("normal", None), ("cool", None), ("archive", None)]),
    enum("ImportFormat", "取り込みの形式", None, [("marc21", None), ("csv", None), ("json", None)]),
    enum("PriorityLevel", "優先度", None, [("low", None), ("normal", None), ("high", None), ("urgent", None)]),
]

LARGE_VALUES = [
    # The value embedded everywhere.
    value("MoneyAmount", "金額", "金額は通貨と最小単位の整数で持つ。浮動小数で持つと延滞金の合計が館ごとにずれるので、ここへ切り出して全部の金額欄が同じ形を使う。",
          [f("currency", emb="CurrencyCode"),
           f("minor_units", "int", d="通貨の最小単位での整数。円なら 1 円単位"),
           f("rounding", emb="RoundingMode", opt=True)]),
    value("PostalAddress", "住所", "郵送と配送に使う住所。行は利用者が書いたとおりに持ち、正規化は送り状を作るときにだけ行う。",
          [f("postal_code", d="ハイフンを含めない 7 桁", rules=[{"kind": "pattern", "regex": "^[0-9]{7}$"}]),
           f("region"), f("city"), f("lines", arr=True), f("address_id", "uuid", opt=True)]),
    value("ContactPoint", "連絡先", None,
          [f("contact_id", "uuid"), f("purpose", emb="ContactPurpose"), f("channel", emb="NotificationChannel"),
           f("address", d="メールアドレス・電話番号・住所のいずれか。channel が決める")]),
    value("PersonName", "人の名前", "姓と名を分けて持つ。読みは窓口で確かめたものだけを入れる。",
          [f("family"), f("given"), f("family_kana", opt=True), f("given_kana", opt=True)]),
    value("DateRange", "期間", "両端を含む期間。終わりの無い期間は until を null にする。",
          [f("since", "datetime"), f("until", "datetime", null=True)]),
    value("TimeWindow", "時間帯", None, [f("opens", d="HH:MM"), f("closes", d="HH:MM")]),
    value("StaffRef", "職員", "記録に残す職員。名前は控えず、職員の id と役割だけを持つ。",
          [f("staff_id", "uuid"), f("role", emb="StaffRole")]),
    value("BranchRef", "館", None, [f("branch_id", "uuid"), f("code", d="館の 3 文字の略号")]),
    value("RecordRef", "記録への参照", "どの種類の記録かと、その記録の番号の組。番号だけでは種類をまたいで重なるので、必ず二つで指す。",
          [f("record_type", d="指す記録の record_type"), f("record_key", d="指す記録の番号")]),
    value("ApprovalStamp", "承認の印", "承認した職員と結果と時刻。承認の欄を持つ記録はどれもこの形を使う。",
          [f("decision", emb="ApprovalDecision"), f("approver", emb="StaffRef"), f("decided_at", "datetime"),
           f("comment", opt=True)]),
    value("Identifier", "識別子", None, [f("scheme", d="isbn・issn・館の登録番号など"), f("identifier_value")]),
    value("ShelfLocation", "棚の位置", None,
          [f("location_id", "uuid"), f("zone", emb="ShelfZone"), f("shelf"), f("position", "int")]),
    value("CallNumber", "請求記号", "分類記号と著者記号の組。分類法が館の独自分類のときは cutter を持たない。",
          [f("scheme", emb="ClassificationScheme"), f("class_number"), f("cutter", opt=True)]),
    value("Contributor", "著者など", None,
          [f("contributor_id", "uuid"), f("name", emb="PersonName"), f("role", d="著・編・訳・絵など")]),
    value("SeriesInfo", "シリーズ", None,
          [f("series_id", "uuid"), f("title"), f("number", opt=True), f("status", emb="SeriesStatus")]),
    value("PhysicalDescription", "形態の記述", None,
          [f("format", emb="ItemFormat"), f("pages", "int", opt=True), f("height_cm", "int", opt=True)]),
    value("SubjectHeading", "件名", None, [f("heading_id", "uuid"), f("heading"), f("scheme")]),
    value("ItemCondition", "資料の状態", "点検と返却のたびに付ける状態。前の状態は消さずに、記録の側に並べて残す。",
          [f("condition", emb="CopyCondition"), f("noted_at", "datetime"), f("noted_by", emb="StaffRef")]),
    value("LoanLine", "貸出の一点", None,
          [f("item_id", "uuid"), f("due_on", "datetime"), f("fee", emb="MoneyAmount", opt=True)]),
    value("ReturnLine", "返却の一点", None,
          [f("item_id", "uuid"), f("returned_at", "datetime"), f("condition", emb="ItemCondition"),
           f("late_fee", emb="MoneyAmount", null=True)]),
    value("PackedItem", "荷の一点", None,
          [f("item_id", "uuid"), f("from_branch", emb="BranchRef"), f("to_branch", emb="BranchRef"),
           f("declared_value", emb="MoneyAmount")]),
    value("PackageSeal", "荷の封", "封の番号と封をした職員。受け取った館は封の番号を確かめてから開ける。",
          [f("seal_id"), f("sealed_by", emb="StaffRef"), f("sealed_at", "datetime")]),
    value("OrderLine", "発注の一行", None,
          [f("line_id", "uuid"), f("title"), f("isbn", opt=True), f("quantity", "int"),
           f("unit_price", emb="MoneyAmount"), f("total", emb="MoneyAmount")],
          rules=[{"kind": "derivedFrom", "field": "total", "from": "unit_price", "note": "単価と数から計算し直して、合わない行を受け付けない"}]),
    value("InvoiceLine", "請求の一行", None,
          [f("order_line_id", "uuid"), f("amount", emb="MoneyAmount"), f("tax", emb="TaxLine")]),
    value("TaxLine", "税", None, [f("tax_id", "uuid"), f("rate_percent", "int"), f("amount", emb="MoneyAmount")]),
    value("PaymentLine", "支払いの一行", None,
          [f("payment_id", "uuid"), f("method", emb="PaymentMethod"), f("amount", emb="MoneyAmount")]),
    value("Discount", "値引き", None, [f("discount_id", "uuid"), f("reason"), f("amount", emb="MoneyAmount")]),
    value("FeeItem", "料金の一項目", None, [f("fee_id", "uuid"), f("label"), f("amount", emb="MoneyAmount")]),
    value("EventSession", "行事の回", None,
          [f("session_id", "uuid"), f("window", emb="TimeWindow"), f("room_id", "uuid"), f("capacity", "int"),
           f("entry_fee", emb="MoneyAmount", null=True)]),
    value("Speaker", "講師", None,
          [f("speaker_id", "uuid"), f("name", emb="PersonName"), f("honorarium", emb="MoneyAmount"),
           f("travel_cost", emb="MoneyAmount", opt=True)]),
    value("Attendance", "参加の数", None, [f("expected", "int"), f("actual", "int", null=True)]),
    value("RoomEquipment", "部屋の設備", None, [f("equipment_id", "uuid"), f("label"), f("rental_fee", emb="MoneyAmount")]),
    value("ShiftSlot", "勤務の枠", None,
          [f("slot_id", "uuid"), f("kind", emb="ShiftKind"), f("window", emb="TimeWindow"), f("staff", emb="StaffRef")]),
    value("CountLine", "点検の一行", None,
          [f("line_id", "uuid"), f("location", emb="ShelfLocation"), f("expected", "int"), f("found", "int")]),
    value("Discrepancy", "点検の食い違い", "点検で見つからなかった資料と、あるはずのない資料。どちらも館の id と資料の id で持つ。",
          [f("item_id", "uuid"), f("branch", emb="BranchRef"), f("missing", "bool"), f("replacement_cost", emb="MoneyAmount")]),
    value("LicenseTerms", "契約の条件", None,
          [f("license", emb="DigitalLicense"), f("seats", "int", null=True), f("price", emb="MoneyAmount"),
           f("period", emb="DateRange")]),
    value("SurveyAnswer", "アンケートの回答", None,
          [f("question_id", "uuid"), f("score", emb="SurveyScale"), f("comment", opt=True)]),
    value("AuditActor", "操作した人", None,
          [f("staff", emb="StaffRef"), f("terminal_id", d="窓口の端末の番号")]),
    value("AuditTarget", "操作の対象", None,
          [f("target_id", "uuid"), f("record", emb="RecordRef"), f("branch", emb="BranchRef")]),
    value("BudgetLine", "予算の一行", "費目ごとの予算。使った額は受け入れのたびに足し、予算を超える発注は承認を要る。",
          [f("line_id", "uuid"), f("period", emb="BudgetPeriod"), f("allocated", emb="MoneyAmount"),
           f("spent", emb="MoneyAmount"), f("reserve", emb="MoneyAmount", opt=True)]),
    value("OpeningHours", "開館時間", None,
          [f("weekday", emb="Weekday"), f("hours", emb="TimeWindow", null=True, d="休館日は null")]),
    value("Holiday", "休館日", None, [f("holiday_id", "uuid"), f("date", "datetime"), f("reason")]),
    value("NoticeTemplate", "お知らせの文面", "知らせる手段と言語ごとの文面。返す日や館の名前は、送るときに文面へ差し込む。",
          [f("template_id", "uuid"), f("channel", emb="NotificationChannel"), f("purpose", emb="ContactPurpose"),
           f("language", emb="LanguageCode"), f("body")]),
    value("RetentionRule", "保存の規則", None,
          [f("rule_id", "uuid"), f("record_kind", emb="RecordKind"), f("keep_days", "int"),
           f("zone", emb="TemperatureZone", opt=True)]),
    value("LoanRule", "貸出の規則", "資料の種類と利用者の区分の組ごとの規則。組は一つにつき一行で、重なる行は受け付けない。",
          [f("rule_id", "uuid"), f("material", emb="MaterialType"), f("member", emb="MemberCategory"),
           f("loan_days", "int"), f("max_items", "int"), f("renewal", emb="RenewalRule"),
           f("daily_fine", emb="MoneyAmount"), f("fine_cap", emb="MoneyAmount")]),
    value("ReservationRule", "予約の規則", None,
          [f("rule_id", "uuid"), f("member", emb="MemberCategory"), f("max_reservations", "int"),
           f("hold_days", "int"), f("pickup", emb="HoldPickup"), f("no_show_fee", emb="MoneyAmount")]),
    value("FineRule", "延滞金の規則", None,
          [f("rule_id", "uuid"), f("reason", emb="FineReason"), f("amount", emb="MoneyAmount"),
           f("cap", emb="MoneyAmount"), f("grace_days", "int")]),
    value("TransferRoute", "配送の経路", "巡回車の経路と回る館の順。順番は荷を積む順でもあるので、並べ替えると積み荷目録が合わなくなる。",
          [f("route_id", "uuid"), f("route", emb="VanRoute"), f("mode", emb="DeliveryMode"),
           f("stops", emb="BranchRef", arr=True, rules=[{"kind": "uniqueBy", "field": "branch_id"}]),
           f("cost_per_trip", emb="MoneyAmount")]),
    value("VendorTerms", "取引の条件", None,
          [f("vendor_id", "uuid"), f("kind", emb="VendorKind"), f("discount_percent", "int"),
           f("shipping_fee", emb="MoneyAmount"), f("free_shipping_over", emb="MoneyAmount", opt=True)]),
    value("MembershipFee", "会費", None,
          [f("member", emb="MemberCategory"), f("annual_fee", emb="MoneyAmount"), f("card_reissue_fee", emb="MoneyAmount")]),
    value("BranchScope", "館の扱える範囲", "館ごとに使ってよい貸出の規則。規則の本体は全体の設定にあり、ここでは id で選ぶだけ。",
          [f("branch_id", "uuid"),
           f("allowed_rule_ids", arr=True, d="使ってよい貸出の規則の id。全体の設定の規則の一覧にあるものだけ",
             pts=("LoanRule", "N:N"),
             rules=[{"kind": "memberOf", "target": "Schemas:NetworkConfiguration.lending_policy.loan_rules[].rule_id"}]),
           f("supervisors", emb="StaffRef", arr=True)],
          lines=[("link", "allowed_rule_ids", "NetworkConfiguration",
                  {"at": "lending_policy.loan_rules[].rule_id", "description": "使ってよい規則の一覧"})]),
    value("LendingPolicy", "貸出の方針", "貸出の規則の一覧と、館ごとの扱える範囲。規則の id は全体で一意で、館の範囲は id で規則を選ぶ。",
          [f("policy_id", "uuid"),
           f("loan_rules", emb="LoanRule", arr=True,
             rules=[{"kind": "sortedBy", "field": "rule_id"}, {"kind": "uniqueBy", "field": "rule_id"}]),
           f("branch_scopes", emb="BranchScope", arr=True), f("default_loan_days", "int"), f("max_renewals", "int")]),
    value("ReservationPolicy", "予約の方針", None,
          [f("policy_id", "uuid"), f("rules", emb="ReservationRule", arr=True), f("queue_limit", "int"),
           f("notice", emb="NoticeTemplate")]),
    value("FinePolicy", "延滞金の方針", "延滞金の規則と上限。上限は一点ごとではなく一回の貸出ごとにかける。",
          [f("policy_id", "uuid"), f("rules", emb="FineRule", arr=True), f("monthly_cap", emb="MoneyAmount"),
           f("waiver_limit", emb="MoneyAmount")]),
    value("TransferPolicy", "配送の方針", None,
          [f("policy_id", "uuid"), f("routes", emb="TransferRoute", arr=True), f("max_items_per_package", "int"),
           f("insurance_threshold", emb="MoneyAmount")]),
    value("AcquisitionPolicy", "受け入れの方針", None,
          [f("policy_id", "uuid"), f("vendors", emb="VendorTerms", arr=True), f("approval_threshold", emb="MoneyAmount"),
           f("methods", emb="AcquisitionMethod", arr=True)]),
    value("MembershipPolicy", "利用登録の方針", None,
          [f("policy_id", "uuid"), f("fees", emb="MembershipFee", arr=True), f("accepted_documents", emb="DocumentKind", arr=True),
           f("valid_years", "int")]),
    value("RetentionPolicy", "保存の方針", None,
          [f("policy_id", "uuid"), f("rules", emb="RetentionRule", arr=True)]),
    value("NotificationPolicy", "お知らせの方針", None,
          [f("policy_id", "uuid"), f("templates", emb="NoticeTemplate", arr=True), f("quiet_hours", emb="TimeWindow"),
           f("postage", emb="MoneyAmount")]),
    value("OpeningPolicy", "開館の方針", None,
          [f("policy_id", "uuid"), f("hours", emb="OpeningHours", arr=True), f("holidays", emb="Holiday", arr=True)]),
    value("CatalogPolicy", "目録の方針", None,
          [f("policy_id", "uuid"), f("scheme", emb="ClassificationScheme"), f("import_formats", emb="ImportFormat", arr=True),
           f("languages", emb="LanguageCode", arr=True)]),
    value("AuditPolicy", "監査の方針", "監査の記録を残す操作と、記録を見てよい範囲。承認の操作は省けない。",
          [f("policy_id", "uuid"), f("actions", emb="AuditAction", arr=True), f("access", emb="AccessLevel"),
           f("retention", emb="RetentionRule")]),
    value("EventPolicy", "行事の方針", None,
          [f("policy_id", "uuid"), f("default_fee", emb="MoneyAmount"), f("speaker_budget", emb="BudgetLine"),
           f("rooms", emb="RoomKind", arr=True)]),
    value("DigitalPolicy", "電子資料の方針", None,
          [f("policy_id", "uuid"), f("default_terms", emb="LicenseTerms"), f("loan_days", "int")]),
    value("PickupChoice", "受け取り方", "取り置きの受け取り方。mode が決める欄だけを持ち、ほかの mode の欄は書かない。",
          [f("mode", emb="HoldPickup"),
           f("branch", emb="BranchRef", opt=True, variants=["branch"]),
           f("locker_number", opt=True, variants=["locker"]),
           f("delivery_address", emb="PostalAddress", opt=True, variants=["home_delivery"]),
           f("delivery_fee", emb="MoneyAmount", opt=True, variants=["home_delivery"])],
          discriminator="mode"),
    value("PaymentChoice", "払い方の選択", None,
          [f("method", emb="PaymentMethod"),
           f("card_last_digits", opt=True, variants=["card"]),
           f("transfer_reference", opt=True, variants=["transfer"]),
           f("waiver_id", "uuid", opt=True, variants=["waiver"])],
          discriminator="method"),
    value("ContactChoice", "連絡の方法", None,
          [f("channel", emb="NotificationChannel"),
           f("email", opt=True, variants=["email"]),
           f("phone", opt=True, variants=["sms"]),
           f("postal", emb="PostalAddress", opt=True, variants=["post"])],
          discriminator="channel"),
    value("CatalogEntry", "索引の一行", "索引の一行。catalog_kind が指す相手を決める。件名・著者・シリーズのどれでも、相手の記録をキーで指す。",
          [f("entry_id", "uuid"), f("catalog_kind", emb="CatalogKind"), f("entry_key"), f("sort_key"), f("target_key")],
          lines=[("byKey", "target_key", "SubjectHeading", {"cardinality": "N:1", "when": {"field": "catalog_kind", "equals": "subject"}}),
                 ("byKey", "target_key", "Contributor", {"cardinality": "N:1", "when": {"field": "catalog_kind", "equals": "author"}}),
                 ("byKey", "target_key", "SeriesInfo", {"cardinality": "N:1", "when": {"field": "catalog_kind", "equals": "series"}})]),
    value("StatusChange", "状態の移り変わり", "記録の状態がどう移ったかと、移した記録。移した記録は種類をまたぐので、種類の欄で相手を選ぶ。",
          [f("change_id", "uuid"), f("from_status"), f("to_status"), f("cause", emb="RecordRef")],
          lines=[("byKey", "cause", None, {"cardinality": "N:1", "discriminatedBy": "record_type"})]),
    value("MemberDocument", "確かめた書類", None,
          [f("kind", emb="DocumentKind"), f("checked_by", emb="StaffRef"), f("checked_at", "datetime"),
           f("document_id", "uuid")]),
    value("MemberPreferences", "利用者の希望", None,
          [f("contact", emb="ContactChoice"), f("language", emb="LanguageCode"), f("pickup", emb="PickupChoice"),
           f("profile_id", "uuid")]),
    value("Guardian", "保護者", None,
          [f("guardian_id", "uuid"), f("name", emb="PersonName"), f("contact", emb="ContactPoint")]),
    value("CardDesign", "カードの図柄", None, [f("design_id", "uuid"), f("label"), f("issue_fee", emb="MoneyAmount")]),
    value("CopySpecifics", "一点ごとの記述", None,
          [f("copy_id", "uuid"), f("barcode"), f("location", emb="ShelfLocation"), f("call_number", emb="CallNumber"),
           f("condition", emb="ItemCondition"), f("price", emb="MoneyAmount")]),
    value("Edition", "版", None,
          [f("edition_id", "uuid"), f("statement"), f("published_year", "int"), f("publisher"),
           f("list_price", emb="MoneyAmount", opt=True)]),
    value("RenewalLine", "延長の一点", None,
          [f("item_id", "uuid"), f("old_due_on", "datetime"), f("new_due_on", "datetime"), f("fee", emb="MoneyAmount")]),
    value("ReceiptLine", "受け入れの一点", None,
          [f("order_line_id", "uuid"), f("received_quantity", "int"), f("cost", emb="MoneyAmount"),
           f("copy", emb="CopySpecifics")]),
    value("FineLine", "延滞金の一項目", None,
          [f("loan_id", "uuid"), f("reason", emb="FineReason"), f("days", "int"), f("amount", emb="MoneyAmount")]),
    value("WaivedLine", "免除の一項目", None, [f("fine_line_id", "uuid"), f("amount", emb="MoneyAmount")]),
    value("BookingSlot", "部屋の予約枠", None,
          [f("slot_id", "uuid"), f("window", emb="TimeWindow"), f("equipment", emb="RoomEquipment", arr=True),
           f("charge", emb="MoneyAmount")]),
    value("WeedingLine", "除籍の一点", None,
          [f("item_id", "uuid"), f("reason", emb="WeedingReason"), f("book_value", emb="MoneyAmount")]),
    value("CountSummary", "点検のまとめ", None,
          [f("summary_id", "uuid"), f("lines", emb="CountLine", arr=True), f("discrepancies", emb="Discrepancy", arr=True),
           f("loss_value", emb="MoneyAmount")]),
    value("LicenseSeat", "電子資料の席", None, [f("seat_id", "uuid"), f("member_id", "uuid", null=True)]),
    value("SurveySection", "アンケートの章", None,
          [f("section_id", "uuid"), f("title"), f("answers", emb="SurveyAnswer", arr=True)]),
    value("AuditChange", "変えた欄", None, [f("field_path"), f("before", "json", null=True), f("after", "json", null=True), f("change_id", "uuid")]),
    value("IndexStats", "索引の数", None, [f("entries", "int"), f("updated_at", "datetime"), f("stats_id", "uuid")]),
    value("TransferException", "配送の例外", None,
          [f("exception_id", "uuid"), f("item", emb="PackedItem"), f("note"), f("priority", emb="PriorityLevel"),
           f("change", emb="StatusChange")]),
    value("ShippingCost", "送料", None,
          [f("route", emb="TransferRoute"), f("base", emb="MoneyAmount"), f("surcharge", emb="MoneyAmount"),
           f("discount", emb="Discount", opt=True), f("cost_id", "uuid")]),
    # Shapes settled ahead of the features that will use them: nothing refers to them yet.
    enum("SeatKind", "閲覧席の種類", "閲覧席の予約を始めるときのための種類。", [("desk", "机"), ("sofa", "ソファ"), ("carrel", "個人ブース"), ("pc", "パソコン席")]),
    enum("DonationCondition", "寄贈の状態", None, [("good", None), ("fair", None), ("poor", None)]),
    enum("ClosureReason", "臨時休館の理由", None, [("maintenance", "点検"), ("weather", "荒天"), ("event", "行事")]),
    value("SeatReservation", "閲覧席の予約", "閲覧席の予約。席の予約を始めるときに使う形で、まだどの記録からも使わない。",
          [f("seat_id", "uuid"), f("kind", emb="SeatKind"), f("window", emb="TimeWindow"), f("member_id", "uuid")]),
    value("MobileStop", "移動図書館の停留所", "移動図書館の車が止まる所と時間帯。巡回の経路に組み込むまでは単独で持つ。",
          [f("stop_id", "uuid"), f("place_name"), f("window", emb="TimeWindow"), f("weekday", emb="Weekday")]),
    value("VolunteerSkill", "ボランティアの得意なこと", None,
          [f("skill_id", "uuid"), f("label"), f("hours_per_month", "int")]),
    value("ExhibitPanel", "展示のパネル", None,
          [f("panel_id", "uuid"), f("title"), f("width_cm", "int"), f("height_cm", "int")]),
    value("RoomRule", "部屋の決まり", None,
          [f("room", emb="RoomKind"), f("max_hours", "int"), f("deposit", emb="MoneyAmount"), f("rule_id", "uuid")]),
]

LARGE_ENTITIES = [
    entity(
        "NetworkConfiguration", "全体の設定",
        "図書館ネットワーク全体の設定。中央館が版を重ねて発行し、各館は受け取った版で貸出・予約・延滞金・配送を判断する。"
        "前の版を番号で指して連なり、同じ版の番号で中身の違うものを受け付けない。方針ごとの欄はそれぞれの値の形に切り出し、"
        "この記録は方針を並べる入れ物にとどめる。館ごとの差は館の扱える範囲（貸出の方針の中）で表し、館ごとの設定の記録は作らない。",
        rec("NetworkConfiguration") + [
            f("network_id", "uuid"),
            f("version", "int"),
            f("previous_configuration_key", d="直前の版の番号。最初の版では null", null=True,
              key=("NetworkConfiguration", "1:1")),
            f("issued_at", "datetime"),
            f("head_office", d="中央館の事務の設定", sub=[
                f("cataloging", d="目録の作り方。版が変わる途中は二つの方式が並ぶ", sub=[
                    f("active_scheme", sub=[
                        f("scheme_id", "uuid"),
                        f("scheme_version", "int", d="方式の版。上がったら各館は目録を作り直す"),
                        f("standard_fee", emb="MoneyAmount", d="一件の目録を外に頼むときの料金"),
                    ]),
                    f("scheme_transition", null=True, d="次の方式へ移る途中だけ持つ。移っていないときは null", sub=[
                        f("next_scheme_id", "uuid"),
                        f("starts_on", "datetime"),
                    ]),
                ]),
                f("account", d="中央館の口座と担当", sub=[
                    f("office_id", const="office:central"),
                    f("contact", emb="ContactPoint"),
                    f("opened_on", "datetime"),
                    f("closes_on", "datetime", opt=True),
                    f("staff_roster", emb="StaffRef", arr=True),
                    f("duty_managers", emb="StaffRef", arr=True),
                    f("budget_lines", emb="BudgetLine", arr=True),
                    f("addresses", emb="PostalAddress", arr=True),
                ]),
            ]),
            f("lending_policy", emb="LendingPolicy"),
            f("reservation_policy", emb="ReservationPolicy"),
            f("fine_policy", emb="FinePolicy"),
            f("transfer_policy", emb="TransferPolicy"),
            f("acquisition_policy", emb="AcquisitionPolicy"),
            f("membership_policy", emb="MembershipPolicy"),
            f("retention_policy", emb="RetentionPolicy"),
            f("notification_policy", emb="NotificationPolicy"),
            f("opening_policy", emb="OpeningPolicy"),
            f("catalog_policy", emb="CatalogPolicy"),
            f("audit_policy", emb="AuditPolicy"),
            f("event_policy", emb="EventPolicy"),
            f("digital_policy", emb="DigitalPolicy"),
        ],
        tags=["監査"], props=["版を重ねる", "前の版を指す"],
        metadata={"owner": "中央館", "versioned": "yes", "purpose": "network-configuration", "status": "確定", "unique_key": "network_id", "approver": "館長"},
        rules=[{"kind": "monotonicPerKey", "keys": ["network_id"], "note": "受け取った版より古い版で置き換えない"},
               {"kind": "chainedBy", "field": "previous_configuration_key", "note": "版の番号の大小だけで比べず、前の版を指しているかを見る"}],
        absent=[{"name": "branch_overrides", "reason": "館ごとの差は貸出の方針の館の扱える範囲で表す"}],
    ),
    entity(
        "BranchRegistry", "館の台帳",
        "ネットワークに属する館の一覧。館を足す・閉じるたびに版を重ね、前の版を指す。閉じた館は一覧から外し、閉じた日は監査の記録に残す。",
        rec("BranchRegistry") + [
            f("registry_id", "uuid"), f("version", "int"),
            f("previous_registry_key", null=True, key=("BranchRegistry", "1:1")),
            f("configuration_key", key=("NetworkConfiguration", "N:1")),
            f("branches", arr=True, sub=[
                f("branch", emb="BranchRef"), f("name"), f("address", emb="PostalAddress"),
                f("contact", emb="ContactPoint"), f("route", emb="VanRoute"),
            ], rules=[{"kind": "sortedBy", "field": "branch.branch_id"}]),
            f("issued_at", "datetime"),
        ],
        tags=["所蔵"], props=["版を重ねる", "前の版を指す"],
        metadata={"owner": "中央館", "versioned": "yes", "purpose": "branch-registry", "status": "確定", "unique_key": "registry_id", "approver": "館長"},
        rules=[{"kind": "monotonicPerKey", "keys": ["registry_id"]},
               {"kind": "chainedBy", "field": "previous_registry_key"},
               {"kind": "uniqueBy", "field": "branches.branch.code"}],
        absent=[{"name": "closed_branches", "reason": "閉じた館は一覧から外し、閉じた日は監査の記録に残す"}],
    ),
    entity(
        "CatalogRecord", "書誌",
        "一つの著作の一つの版の書誌。所蔵の一点はこの書誌を番号で指す。取り込んだ書誌は形式を問わずこの形に直してから持ち、元の形は持たない。",
        rec("CatalogRecord") + [
            f("catalog_id", "uuid", pk=True), f("title"), f("title_kana", opt=True),
            f("identifiers", emb="Identifier", arr=True), f("contributors", emb="Contributor", arr=True),
            f("edition", emb="Edition"), f("series", emb="SeriesInfo", opt=True),
            f("subjects", emb="SubjectHeading", arr=True), f("call_number", emb="CallNumber"),
            f("physical", emb="PhysicalDescription"), f("language", emb="LanguageCode"),
            f("material", emb="MaterialType"), f("access", emb="AccessLevel"),
            f("imported_from", emb="ImportFormat", opt=True), f("created_at", "datetime"),
        ],
        tags=["目録"], props=[],
        metadata={"owner": "目録係", "versioned": "no", "purpose": "catalog-record", "status": "確定", "unique_key": "catalog_id", "approver": "司書"},
        rules=[{"kind": "uniqueBy", "field": "identifiers", "keys": ["scheme", "identifier_value"]}],
        absent=[{"name": "source_record", "reason": "取り込んだ元の形は持たない"}],
    ),
    entity(
        "HoldingItem", "所蔵の一点",
        "棚に並ぶ一点。書誌を番号で指し、どの館の棚にあるかと状態を持つ。館間で配送しても所属の館は変えない。",
        rec("HoldingItem") + [
            f("item_id", "uuid", pk=True), f("catalog_key", key=("CatalogRecord", "N:1")),
            f("home_branch", emb="BranchRef"), f("copy", emb="CopySpecifics"),
            f("acquired_by", emb="AcquisitionMethod"), f("acquired_at", "datetime"),
            f("conditions", emb="ItemCondition", arr=True, rules=[{"kind": "sortedBy", "field": "noted_at"}]),
            f("storage", emb="TemperatureZone"),
        ],
        tags=["所蔵"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "holding-item", "status": "確定", "unique_key": "item_id", "approver": "司書"},
        rules=[{"kind": "gteRef", "field": "acquired_at", "target": "Schemas:CatalogRecord.created_at", "note": "書誌より先に受け入れない"}],
        absent=[{"name": "current_branch", "reason": "今どこにあるかは配送の記録から引く"}],
    ),
    entity(
        "MemberAccount", "利用者",
        "利用登録をした人。区分と状態で貸出の上限が決まる。子どもの登録は保護者を持ち、保護者の連絡先にも知らせる。退会した人の記録は保存の規則に従って消す。",
        rec("MemberAccount") + [
            f("member_id", "uuid", pk=True), f("name", emb="PersonName"), f("category", emb="MemberCategory"),
            f("status", emb="MemberStatus"), f("home_branch", emb="BranchRef"),
            f("address", emb="PostalAddress"), f("contacts", emb="ContactPoint", arr=True),
            f("preferences", emb="MemberPreferences"), f("guardian", emb="Guardian", null=True),
            f("documents", emb="MemberDocument", arr=True), f("registered_at", "datetime"),
            f("expires_at", "datetime"),
        ],
        tags=["利用者"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "member-account", "status": "確定", "unique_key": "member_id", "approver": "司書"},
        rules=[{"kind": "requiredIf", "when": "category == child", "then": "guardian"},
               {"kind": "gteRef", "field": "expires_at", "target": "Schemas:MemberAccount.registered_at"}],
        absent=[{"name": "password", "reason": "利用者の認証は別の仕組みが持つ"}],
    ),
    entity(
        "MemberCard", "利用者カード",
        "発行した利用者カード。再発行すると前のカードの番号を指し、前のカードは使えなくなる。",
        rec("MemberCard") + [
            f("card_id", "uuid"), f("member_key", key=("MemberAccount", "N:1")), f("card_number"),
            f("previous_card_key", null=True, key=("MemberCard", "1:1")), f("design", emb="CardDesign"),
            f("check_digits", d="カードの番号の末尾。読み取り機が確かめる",
              rules=[{"kind": "pattern", "regex": "^(?!00)[0-9]{2}$", "note": "読み取り機の正規表現で書いた。00 は使わない"}]),
            f("issued_at", "datetime"), f("issued_by", emb="StaffRef"),
        ],
        tags=["利用者"], props=["前の版を指す"],
        metadata={"owner": "各館", "versioned": "yes", "purpose": "member-card", "status": "確定", "unique_key": "card_number", "approver": "司書"},
        rules=[{"kind": "chainedBy", "field": "previous_card_key"},
               {"kind": "pattern", "field": "card_number", "regex": "^(?!0{10})[0-9]{10}$"}],
        absent=[{"name": "pin", "reason": "暗証番号は持たない"}],
    ),
    entity(
        "LoanRequest", "貸出の申し込み",
        "窓口や自動貸出機で受けた貸出の申し込み。規則に照らして通ったものだけが貸出になる。申し込み自体はどこにも埋め込まれず、貸出の記録が番号で指す。",
        rec("LoanRequest") + [
            f("request_id", "uuid"), f("member_key", key=("MemberAccount", "N:1")),
            f("items", arr=True, sub=[f("item_key", key=("HoldingItem", "N:1")), f("requested_days", "int", opt=True)]),
            f("branch", emb="BranchRef"), f("received_at", "datetime"), f("terminal_id"),
        ],
        tags=["貸出"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "loan-request", "status": "下書き", "unique_key": "request_id", "approver": "なし"},
        rules=[{"kind": "uniqueBy", "field": "items.item_key"}],
        absent=[{"name": "decision", "reason": "通ったかどうかは貸出の記録があるかで言う"}],
    ),
    entity(
        "LoanRecord", "貸出",
        "貸した記録。申し込みを番号で指し、使った貸出の規則を id で持つ。延長はこの記録を書き換えず、延長の記録を足す。",
        rec("LoanRecord") + [
            f("loan_id", "uuid", pk=True), f("request_key", key=("LoanRequest", "1:1")),
            f("member_key", key=("MemberAccount", "N:1")),
            f("rule_id", pts=("LoanRule", "N:1"), d="使った貸出の規則"),
            f("lines", emb="LoanLine", arr=True), f("status", emb="LoanStatus"),
            f("branch", emb="BranchRef"), f("lent_at", "datetime"), f("lent_by", emb="StaffRef"),
        ],
        tags=["貸出"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "loan-record", "status": "確定", "unique_key": "loan_id", "approver": "司書"},
        rules=[{"kind": "uniqueBy", "field": "lines", "keys": ["item_id"]},
               {"kind": "gteRef", "field": "lent_at", "target": "Schemas:LoanRequest.received_at"}],
        absent=[{"name": "returned_at", "reason": "返却は返却の記録が持つ"}],
        lines=[("link", "rule_id", "NetworkConfiguration", {"at": "lending_policy.loan_rules[].rule_id", "description": "使ってよい規則の一覧"})],
    ),
    entity(
        "ReturnRecord", "返却",
        "返された記録。貸出を番号で指し、一点ごとの状態と延滞金を持つ。返却ポストに入ったものは開館後に受けた時刻で記録する。",
        rec("ReturnRecord") + [
            f("return_id", "uuid"), f("loan_key", key=("LoanRecord", "N:1")),
            f("lines", emb="ReturnLine", arr=True), f("branch", emb="BranchRef"),
            f("received_at", "datetime"), f("received_by", emb="StaffRef"),
        ],
        tags=["貸出"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "return-record", "status": "確定", "unique_key": "return_id", "approver": "司書"},
        rules=[{"kind": "gteRef", "field": "received_at", "target": "Schemas:LoanRecord.lent_at"}],
        absent=[{"name": "fine_paid", "reason": "支払いは支払いの記録が持つ"}],
    ),
    entity(
        "RenewalRecord", "延長",
        "貸出の期限を延ばした記録。延長できる回数は貸出の規則が決める。",
        rec("RenewalRecord") + [
            f("renewal_id", "uuid"), f("loan_key", key=("LoanRecord", "N:1")),
            f("lines", emb="RenewalLine", arr=True), f("renewed_at", "datetime"),
        ],
        tags=["貸出"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "renewal-record", "status": "確定", "unique_key": "renewal_id", "approver": "なし"},
        rules=[{"kind": "lteRef", "field": "lines", "target": "Schemas:NetworkConfiguration.lending_policy.max_renewals"}],
        absent=[{"name": "approver", "reason": "延長は規則の範囲で自動で通す"}],
    ),
    entity(
        "Reservation", "予約",
        "資料の予約。順番が来たら取り置き、受け取りの期限までに来なければ次の人へ回す。受け取り方は利用者の希望から写し、予約の後で希望が変わってもこの記録は変えない。",
        rec("Reservation") + [
            f("reservation_id", "uuid"), f("member_key", key=("MemberAccount", "N:1")),
            f("catalog_key", key=("CatalogRecord", "N:1")),
            f("rule_id", pts=("ReservationRule", "N:1")),
            f("status", emb="ReservationStatus"), f("pickup", emb="PickupChoice"),
            f("queue_position", "int"), f("placed_at", "datetime"), f("expires_at", "datetime", null=True),
        ],
        tags=["予約"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "reservation", "status": "確定", "unique_key": "reservation_id", "approver": "なし"},
        rules=[{"kind": "absentIf", "when": "status == waiting", "then": "expires_at"}],
        absent=[{"name": "item_id", "reason": "予約は書誌に付け、どの一点を渡すかは取り置きのときに決める"}],
        lines=[("link", "rule_id", "NetworkConfiguration", {"at": "reservation_policy.rules[].rule_id", "description": "使ってよい規則の一覧"})],
    ),
    entity(
        "HoldShelfNotice", "取り置きの知らせ",
        "取り置いたことを利用者に知らせた記録。知らせた手段と文面を持ち、届かなかったときは次の手段で知らせ直す。",
        rec("HoldShelfNotice") + [
            f("notice_id", "uuid"), f("reservation_key", key=("Reservation", "N:1")),
            f("template", emb="NoticeTemplate"), f("sent_at", "datetime"), f("delivered", "bool"),
            f("change", emb="StatusChange"),
        ],
        tags=["予約"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "hold-shelf-notice", "status": "確定", "unique_key": "notice_id", "approver": "なし"},
        rules=[{"kind": "requiredIf", "when": "delivered == false", "then": "change"}],
        absent=[{"name": "body_text", "reason": "文面は雛形から作り直せるので控えない"}],
    ),
    entity(
        "TransferManifest", "積み荷目録",
        "館間配送の荷の目録。荷を積んだ館が作り、受け取った館は目録と荷を突き合わせる。目録はどこにも埋め込まれず、受け取りの記録が番号で指す。",
        rec("TransferManifest") + [
            f("manifest_id", "uuid"), f("route", emb="TransferRoute"),
            f("route_id", pts=("TransferRoute", "N:1"), d="使った配送の経路"),
            f("packages", arr=True, sub=[
                f("package_id", "uuid"), f("seal", emb="PackageSeal"), f("items", emb="PackedItem", arr=True),
                f("priority", emb="PriorityLevel"),
            ]), f("status", emb="TransferStatus"), f("loaded_at", "datetime"), f("loaded_by", emb="StaffRef"),
        ],
        tags=["館間配送"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "transfer-manifest", "status": "確定", "unique_key": "manifest_id", "approver": "司書"},
        rules=[{"kind": "uniqueBy", "field": "packages.seal.seal_id"},
               {"kind": "lteRef", "field": "packages.items", "target": "Schemas:NetworkConfiguration.transfer_policy.max_items_per_package"}],
        absent=[{"name": "driver", "reason": "運転する人は配送の委託先が持つ"}],
        lines=[("link", "route_id", "NetworkConfiguration", {"at": "transfer_policy.routes[].route_id", "description": "使ってよい経路の一覧"})],
    ),
    entity(
        "TransferReceipt", "荷の受け取り",
        "荷を受け取った記録。目録を番号で指し、数が合わないものを例外として持つ。",
        rec("TransferReceipt") + [
            f("receipt_id", "uuid"), f("manifest_key", key=("TransferManifest", "N:1")),
            f("received_at", "datetime"), f("received_by", emb="StaffRef"),
            f("exceptions", emb="TransferException", arr=True), f("cost", emb="ShippingCost"),
        ],
        tags=["館間配送"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "transfer-receipt", "status": "確定", "unique_key": "receipt_id", "approver": "司書"},
        rules=[{"kind": "gteRef", "field": "received_at", "target": "Schemas:TransferManifest.loaded_at"}],
        absent=[{"name": "photos", "reason": "荷の写真は持たない"}],
    ),
    entity(
        "PurchaseOrder", "発注",
        "取引先への発注。予算の行を id で選び、承認の金額を超える発注は承認の印を要る。一部だけ届いた発注は残りが届くまで閉じない。",
        rec("PurchaseOrder") + [
            f("order_id", "uuid", pk=True), f("vendor", emb="VendorTerms"),
            f("budget_line_id", pts=("BudgetLine", "N:1")),
            f("lines", emb="OrderLine", arr=True), f("status", emb="OrderStatus"),
            f("approval", emb="ApprovalStamp", null=True), f("ordered_at", "datetime"),
            f("total", emb="MoneyAmount"),
        ],
        tags=["受け入れ"], props=["承認が要る"],
        metadata={"owner": "受け入れ係", "versioned": "no", "purpose": "purchase-order", "status": "確定", "unique_key": "order_id", "approver": "館長"},
        rules=[{"kind": "requiredIf", "when": "total > approval_threshold", "then": "approval"},
               {"kind": "derivedFrom", "field": "total", "from": "lines.total"}],
        absent=[{"name": "invoice", "reason": "請求は請求書の記録が持つ"}],
        lines=[("link", "budget_line_id", "NetworkConfiguration", {"at": "head_office.account.budget_lines[].line_id", "description": "使ってよい予算の行"})],
    ),
    entity(
        "GoodsReceipt", "納品の受け入れ",
        "届いた資料を受け入れた記録。発注を番号で指し、受け入れた一点ごとに所蔵の一点を作る。",
        rec("GoodsReceipt") + [
            f("receipt_id", "uuid"), f("order_key", key=("PurchaseOrder", "N:1")),
            f("lines", emb="ReceiptLine", arr=True), f("received_at", "datetime"), f("received_by", emb="StaffRef"),
        ],
        tags=["受け入れ"], props=[],
        metadata={"owner": "受け入れ係", "versioned": "no", "purpose": "goods-receipt", "status": "確定", "unique_key": "receipt_id", "approver": "司書"},
        rules=[{"kind": "gteRef", "field": "received_at", "target": "Schemas:PurchaseOrder.ordered_at"}],
        absent=[{"name": "invoice_number", "reason": "請求書の番号は請求書の記録が持つ"}],
    ),
    entity(
        "VendorInvoice", "請求書",
        "取引先からの請求。発注の行ごとに金額と税を持ち、受け入れと突き合わせてから払う。",
        rec("VendorInvoice") + [
            f("invoice_id", "uuid"), f("order_key", key=("PurchaseOrder", "N:1")),
            f("lines", emb="InvoiceLine", arr=True), f("shipping", emb="MoneyAmount"),
            f("total", emb="MoneyAmount"), f("due_on", "datetime"), f("payment", emb="PaymentChoice"),
        ],
        tags=["受け入れ"], props=["承認が要る"],
        metadata={"owner": "受け入れ係", "versioned": "no", "purpose": "vendor-invoice", "status": "確定", "unique_key": "invoice_id", "approver": "館長"},
        rules=[{"kind": "derivedFrom", "field": "total", "from": "lines.amount"}],
        absent=[{"name": "bank_account", "reason": "振込先は会計の仕組みが持つ"}],
    ),
    entity(
        "FineAssessment", "延滞金の決定",
        "延滞・汚損・紛失に対して延滞金を決めた記録。貸出を番号で指し、規則の上限をかけてから合計を出す。",
        rec("FineAssessment") + [
            f("assessment_id", "uuid"), f("loan_key", key=("LoanRecord", "N:1")),
            f("member_key", key=("MemberAccount", "N:1")),
            f("rule_ids", arr=True, pts=("FineRule", "N:N"), d="当てた延滞金の規則"),
            f("lines", emb="FineLine", arr=True), f("total", emb="MoneyAmount"),
            f("assessed_at", "datetime"), f("assessed_by", emb="StaffRef"),
        ],
        tags=["貸出", "監査"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "fine-assessment", "status": "確定", "unique_key": "assessment_id", "approver": "司書"},
        rules=[{"kind": "derivedFrom", "field": "total", "from": "lines.amount"}],
        absent=[{"name": "paid", "reason": "払われたかは支払いの記録で言う"}],
        lines=[("link", "rule_ids", "NetworkConfiguration", {"at": "fine_policy.rules[].rule_id", "description": "使ってよい規則の一覧"})],
    ),
    entity(
        "PaymentRecord", "支払い",
        "延滞金・会費・料金の支払い。一回の支払いで複数の項目を払える。",
        rec("PaymentRecord") + [
            f("payment_id", "uuid"), f("member_key", key=("MemberAccount", "N:1")),
            f("lines", emb="PaymentLine", arr=True), f("fees", emb="FeeItem", arr=True),
            f("discounts", emb="Discount", arr=True), f("paid_at", "datetime"), f("received_by", emb="StaffRef"),
        ],
        tags=["利用者"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "payment-record", "status": "確定", "unique_key": "payment_id", "approver": "なし"},
        rules=[{"kind": "uniqueBy", "field": "lines", "keys": ["payment_id"]}],
        absent=[{"name": "card_number", "reason": "カードの番号は持たない"}],
    ),
    entity(
        "FineWaiver", "延滞金の免除",
        "延滞金を免除した記録。免除の上限を超えるものは館長の承認を要る。",
        rec("FineWaiver") + [
            f("waiver_id", "uuid"), f("assessment_key", key=("FineAssessment", "N:1")),
            f("lines", emb="WaivedLine", arr=True), f("reason"), f("approval", emb="ApprovalStamp"),
        ],
        tags=["監査"], props=["承認が要る"],
        metadata={"owner": "各館", "versioned": "no", "purpose": "fine-waiver", "status": "確定", "unique_key": "waiver_id", "approver": "館長"},
        rules=[{"kind": "lteRef", "field": "lines", "target": "Schemas:NetworkConfiguration.fine_policy.waiver_limit"}],
        absent=[{"name": "member_comment", "reason": "利用者の申し出は窓口の控えに残す"}],
    ),
    entity(
        "EventPlan", "行事の計画",
        "読み聞かせ・工作・展示・講演の計画。回ごとに部屋と時間帯を持ち、講師への謝礼は予算の行から出す。",
        rec("EventPlan") + [
            f("event_id", "uuid"), f("kind", emb="EventKind"), f("title"), f("audience", emb="AudienceAge"),
            f("sessions", emb="EventSession", arr=True), f("speakers", emb="Speaker", arr=True),
            f("attendance", emb="Attendance"), f("budget", emb="BudgetLine"), f("branch", emb="BranchRef"),
        ],
        tags=["利用者"], props=["承認が要る"],
        metadata={"owner": "各館", "versioned": "no", "purpose": "event-plan", "status": "下書き", "unique_key": "event_id", "approver": "館長"},
        rules=[{"kind": "sortedBy", "field": "sessions", "keys": ["window.opens"]}],
        absent=[{"name": "flyer", "reason": "ちらしは別に作る"}],
    ),
    entity(
        "RoomBooking", "部屋の予約",
        "学習室・会議室・ホールの予約。部屋の決まりに照らし、預かり金を受けてから確定する。",
        rec("RoomBooking") + [
            f("booking_id", "uuid"), f("room_id", "uuid"), f("rule", emb="RoomRule"),
            f("member_key", key=("MemberAccount", "N:1")), f("slots", emb="BookingSlot", arr=True),
            f("status", emb="BookingStatus"),
        ],
        tags=["利用者"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "room-booking", "status": "確定", "unique_key": "booking_id", "approver": "なし"},
        rules=[{"kind": "requiredIf", "when": "status == confirmed", "then": "slots"}],
        absent=[{"name": "purpose_text", "reason": "使い道は聞かない"}],
    ),
    entity(
        "StaffShift", "勤務表",
        "館ごとの一週間の勤務表。枠ごとに職員を一人割り当てる。",
        rec("StaffShift") + [
            f("roster_id", "uuid"), f("branch", emb="BranchRef"), f("week_of", "datetime"),
            f("slots", emb="ShiftSlot", arr=True), f("approved", emb="ApprovalStamp", null=True),
        ],
        tags=["監査"], props=["承認が要る"],
        metadata={"owner": "各館", "versioned": "no", "purpose": "staff-shift", "status": "確定", "unique_key": "roster_id", "approver": "館長"},
        rules=[{"kind": "uniqueBy", "field": "slots", "keys": ["slot_id"]}],
        absent=[{"name": "wages", "reason": "給与は別の仕組みが持つ"}],
    ),
    entity(
        "WeedingDecision", "除籍の決定",
        "所蔵から外す資料を決めた記録。理由と帳簿の価額を一点ごとに持ち、館長の承認を要る。",
        rec("WeedingDecision") + [
            f("decision_id", "uuid"), f("branch", emb="BranchRef"), f("lines", emb="WeedingLine", arr=True),
            f("approval", emb="ApprovalStamp"), f("decided_at", "datetime"),
        ],
        tags=["所蔵", "監査"], props=["承認が要る"],
        metadata={"owner": "各館", "versioned": "no", "purpose": "weeding-decision", "status": "確定", "unique_key": "decision_id", "approver": "館長"},
        rules=[{"kind": "uniqueBy", "field": "lines", "keys": ["item_id"]}],
        absent=[{"name": "buyer", "reason": "払い下げの相手は持たない"}],
    ),
    entity(
        "InventoryCount", "蔵書点検",
        "棚ごとに数えた蔵書点検の記録。見つからない資料と、あるはずのない資料を食い違いとして持つ。",
        rec("InventoryCount") + [
            f("count_id", "uuid"), f("branch", emb="BranchRef"), f("summary", emb="CountSummary"),
            f("counted_from", "datetime"), f("counted_until", "datetime"), f("lead", emb="StaffRef"),
        ],
        tags=["所蔵"], props=[],
        metadata={"owner": "各館", "versioned": "no", "purpose": "inventory-count", "status": "確定", "unique_key": "count_id", "approver": "司書"},
        rules=[{"kind": "gteRef", "field": "counted_until", "target": "Schemas:InventoryCount.counted_from"}],
        absent=[{"name": "scanner_log", "reason": "読み取り機の記録は持たない"}],
    ),
    entity(
        "DigitalLicenseGrant", "電子資料の契約",
        "電子資料の利用の契約。席ごとに今使っている利用者を持ち、契約の期間が過ぎたら席を空ける。",
        rec("DigitalLicenseGrant") + [
            f("grant_id", "uuid"), f("catalog_key", key=("CatalogRecord", "N:1")), f("terms", emb="LicenseTerms"),
            f("seats", emb="LicenseSeat", arr=True), f("vendor", emb="VendorTerms"),
        ],
        tags=["受け入れ"], props=[],
        metadata={"owner": "受け入れ係", "versioned": "no", "purpose": "digital-license-grant", "status": "確定", "unique_key": "grant_id", "approver": "館長"},
        rules=[{"kind": "lteRef", "field": "seats", "target": "Schemas:DigitalLicenseGrant.terms.seats"}],
        absent=[{"name": "drm_key", "reason": "利用の制御は配信の委託先が持つ"}],
    ),
    entity(
        "SurveyResponse", "アンケート",
        "利用者アンケートの回答。名前は聞かず、利用者の区分と館だけを持つ。",
        rec("SurveyResponse") + [
            f("response_id", "uuid"), f("branch", emb="BranchRef"), f("category", emb="MemberCategory"),
            f("sections", emb="SurveySection", arr=True), f("answered_at", "datetime"),
        ],
        tags=["利用者"], props=[],
        metadata={"owner": "中央館", "versioned": "no", "purpose": "survey-response", "status": "確定", "unique_key": "response_id", "approver": "なし"},
        rules=[{"kind": "uniqueBy", "field": "sections", "keys": ["section_id"]}],
        absent=[{"name": "member_id", "reason": "誰の回答かは持たない"}],
    ),
    entity(
        "AuditEntry", "監査の記録",
        "記録への操作を一件ずつ残す監査の記録。前の記録のハッシュを持って連なり、途中を消すと連なりが切れて分かる。"
        "指す記録は種類をまたぐので、種類の欄で相手を選ぶ。",
        rec("AuditEntry") + [
            f("entry_id", "uuid"), f("sequence", "int"),
            f("previous_entry_hash", null=True, rules=[{"kind": "pattern", "regex": "^sha256:[A-Za-z0-9_-]+$"}]),
            f("occurred_at", "datetime"), f("action", emb="AuditAction"), f("actor", emb="AuditActor"),
            f("target", emb="AuditTarget"), f("records", emb="RecordRef", arr=True),
            f("changes", emb="AuditChange", arr=True), f("detail", "json", opt=True),
        ],
        tags=["監査"], props=["前の版を指す"],
        metadata={"owner": "中央館", "versioned": "no", "purpose": "audit-entry", "status": "確定", "unique_key": "entry_id", "approver": "なし"},
        rules=[{"kind": "chainedBy", "field": "previous_entry_hash"},
               {"kind": "monotonicPerKey", "keys": ["sequence"]}],
        absent=[{"name": "ip_address", "reason": "端末の番号で足りる"}],
        lines=[("byKey", "records", None, {"cardinality": "N:1", "discriminatedBy": "record_type"})],
    ),
    entity(
        "CatalogIndex", "索引",
        "件名・著者・シリーズの索引。行ごとに索引の種類を持ち、種類に応じて指す相手が変わる。",
        rec("CatalogIndex") + [
            f("index_id", "uuid"), f("entries", emb="CatalogEntry", arr=True,
                                     rules=[{"kind": "sortedBy", "field": "sort_key"}]),
            f("stats", emb="IndexStats"), f("built_at", "datetime"),
        ],
        tags=["目録"], props=["版を重ねる"],
        metadata={"owner": "目録係", "versioned": "yes", "purpose": "catalog-index", "status": "確定", "unique_key": "index_id", "approver": "司書"},
        rules=[{"kind": "sortedBy", "field": "entries", "keys": ["catalog_kind"]}],
        absent=[{"name": "full_text", "reason": "本文の索引は持たない"}],
    ),
]


def record_meta():
    """What every record of the network ends with: who made it and changed it, and when."""
    return f("record_meta", d="記録そのものの控え。中身の欄とは別に、作った・変えた職員と時刻を持つ", sub=[
        f("created_at", "datetime"), f("created_by", "uuid"), f("updated_at", "datetime"),
        f("updated_by", "uuid", null=True), f("revision", "int", d="変えるたびに 1 ずつ上げる"),
        f("source_terminal", opt=True), f("note", opt=True),
    ])


for _card in LARGE_ENTITIES:
    _card["fields"].append(record_meta())

LARGE_TAG_GROUPS = [
    {"name": "業務", "tags": ["目録", "所蔵", "利用者", "貸出", "予約", "館間配送", "受け入れ", "監査"], "exclusive": False},
    {"name": "性質", "tags": ["版を重ねる", "前の版を指す", "承認が要る"], "exclusive": False},
    {"name": "貸出の流れ", "tags": ["予約", "貸出"], "exclusive": False},
]

BYKEY_LINE = {
    "line": "byKey",
    "label": "キーで指す",
    "description": "値が相手をその記録の番号（キー）で指す。値の型は変わらず、相手は埋め込まれない",
    "base": "designate",
    "look": {"color": "blue", "dash": "solid", "begin": "none", "end": "arrow"},
    "from": [{"kinds": ["entity", "value"], "at": "row"}],
    "to": {"kinds": ["*"], "at": ["card", "row"]},
    "max": None,
    "fields": [
        {"name": "cardinality", "label": "数", "fieldtype": "dropdownlist", "options": ["1:1", "1:N", "N:1", "N:N"]},
        {"name": "when", "label": "この相手になる条件", "fieldtype": "group", "fields": [
            {"name": "field", "fieldtype": "text", "required": True, "label": "判別子のフィールド"},
            {"name": "equals", "fieldtype": "text", "required": True, "label": "選ばれる値"},
        ]},
        {"name": "discriminatedBy", "fieldtype": "text", "label": "相手の種類名が入っている欄"},
        {"name": "discriminatorField", "fieldtype": "text", "label": "相手の側で const を持つ欄"},
    ],
    "export": {"as": "annotation", "key": "x-pointsAt"},
}


# ---------------------------------------------------------------------------------------------
# The data model: a library's lending, four enums and five entities.

DATAMODEL_CARDS = [
    enum("ItemKind", "資料の種別", None, [("book", "図書"), ("magazine", "雑誌"), ("disc", "ディスク")]),
    enum("ItemGenre", "ジャンル", None,
         [("novel", "小説"), ("essay", "随筆"), ("poetry", "詩"), ("history", "歴史"), ("science", "科学"),
          ("art", "美術"), ("music", "音楽"), ("travel", "旅行"), ("cooking", "料理"), ("children", "児童"),
          ("comics", "まんが"), ("reference", "参考図書"), ("language", "語学"), ("sports", "スポーツ")]),
    enum("ShelfSize", "棚の大きさ", None, [("small", None), ("medium", None), ("large", None), ("oversize", None)]),
    enum("LogTarget", "記録の対象の種別", None,
         [("library", "図書館"), ("branch", "分館"), ("item", "資料"), ("member", "利用者"), ("loan", "貸出")]),
    entity("Library", "図書館", "分館をまとめる単位。資料の貸し借りは図書館をまたげない。",
           [f("id", "uuid", pk=True), f("name", label="館の名前"),
            f("slug", d="名前から作る", unique=True), f("description", "text", opt=True),
            f("openedAt", "datetime"), f("updatedAt", "datetime", d="変わるたびに書き直す"),
            f("branches", label="分館", arr=True, opt=True, emb="Branch")]),
    entity("Branch", "分館", "図書館の中のひとつの館。得意な資料の種別を一つ持つ。",
           [f("id", "uuid", pk=True), f("library", emb="Library"),
            f("name", d="図書館の中で一意。館の札の名前になる"), f("label", label="表示名", opt=True),
            f("focus", emb="ItemKind"), f("description", "text", opt=True),
            f("mapPosition", label="地図での位置", d="案内図の表示のためだけに持ち、変更の記録に残さない", opt=True, sub=[
                f("posX", "float", opt=True), f("posY", "float", opt=True),
                f("pinned", "bool", d="手で置いた。自動の配置で動かさない", opt=True)]),
            f("sortOrder", "int"),
            f("items", label="資料", arr=True, opt=True, emb="Item")]),
    entity("Item", "資料", "分館の持ち物。parent でまとめ物（全集・セット）の中の一冊を表し、入れ子の深さに限りはない。",
           [f("id", "uuid", pk=True), f("branch", d="入れ子の深さによらず、置いてある館を指す", emb="Branch"),
            f("parent", label="まとめ物", d="null なら単独の資料。全集の一冊はこれで表す", opt=True, emb="Item"),
            f("title", d="同じ館で一意"), f("label", label="表示名", opt=True),
            f("genre", emb="ItemGenre"),
            f("homeBranch", label="返す館", d="ほかの館から借りたときだけ持つ", opt=True, emb="Branch"),
            f("size", opt=True, emb="ShelfSize"),
            f("loanTerms", label="貸出の条件", opt=True, sub=[
                f("isReference", "bool", opt=True), f("isRenewable", "bool", opt=True),
                f("isReservable", "bool", opt=True), f("isOversize", "bool", opt=True)]),
            f("note", "text", label="備考", opt=True), f("sortOrder", "int")]),
    entity("Member", "利用者", "分館で登録した利用者。",
           [f("id", "uuid", pk=True), f("branch", emb="Branch"), f("cardNumber"),
            f("name", opt=True), f("sortOrder", "int")]),
    entity("ActivityLog", "活動の記録", "貸し出しや登録のたびに一行ずつ足す記録。書き換えず、消さない。",
           [f("id", "uuid", pk=True), f("library", emb="Library"), f("at", "datetime"),
            f("kind", d="item.lend / member.register など"),
            f("target", label="記録の対象", opt=True, sub=[
                f("type", emb="LogTarget"), f("id", "uuid", opt=True)]),
            f("summary", d="画面にそのまま出す一行"),
            f("payload", "json", d="そのときの中身の写し", opt=True)]),
]

DATAMODEL_TAGS = {
    "ItemKind": ["語彙"], "ItemGenre": ["語彙"], "ShelfSize": ["語彙"], "LogTarget": ["語彙", "履歴"],
    "Library": ["中核"], "Branch": ["中核"], "Item": ["中核"], "Member": ["中核", "語彙"], "ActivityLog": ["履歴"],
}

SMALL_CARDS = [
    entity("Memo", None, None, [
        f("field1", opt=True, sub=[
            f("field1", opt=True), f("field2", opt=True),
            f("field3", opt=True, sub=[f("field1", opt=True), f("field2", opt=True)]),
        ]),
    ], lines=[("embed", "field1.field1", "Memo", None)]),
]


# ---------------------------------------------------------------------------------------------
# Writing a project file.


class Project:
    def __init__(self, key, name, slug, description, created_at, tag_groups, cards, line_kinds=None,
                 card_tags=None):
        self.key = key
        self.name = name
        self.slug = slug
        self.description = description
        self.created_at = created_at
        self.tag_groups = tag_groups
        self.cards = cards
        self.line_kinds = line_kinds or []
        self.card_tags = card_tags or {}
        self.ids = {c["name"]: uid(key, "card", c["name"]) for c in cards}
        self.dirs = [(uid(key, "dir", n), n) for n in ("ts", "zod", "schema")]

    def id(self, *parts):
        return uid(self.key, *parts)

    def field_rows(self, card, fields, prefix, connections, required_default=True):
        rows = []
        for spec in fields:
            path = prefix + [spec["name"]]
            dotted = ".".join(path)
            body = {"name": spec["name"]}
            if spec["label"]:
                body["label"] = spec["label"]
            if spec["d"]:
                body["description"] = spec["d"]
            if spec["type"]:
                body["type"] = spec["type"]
            if spec["arr"]:
                body["array"] = True
            if spec["req"] and required_default:
                body["required"] = True
            if spec["null"]:
                body["nullable"] = True
            if spec["unique"]:
                body["unique"] = True
            if spec["const"] is not None:
                body["const"] = spec["const"]
            if spec["pk"]:
                body["primaryKey"] = True
            if spec["rules"]:
                body["rules"] = spec["rules"]
            if spec["variants"]:
                body["variants"] = spec["variants"]
            row = {"id": self.id("field", card, dotted), "fields": body}
            if spec["sub"] is not None:
                row["rows"] = {"fields": self.field_rows(card, spec["sub"], path, connections, required_default)}
            if spec["emb"]:
                connections.append(self.connection(card, "embed", dotted, spec["emb"], None))
            if spec["pts"]:
                target, cardinality = spec["pts"]
                connections.append(self.connection(card, "points", dotted, target, {"cardinality": cardinality}))
            if spec["key"]:
                target, cardinality = spec["key"]
                connections.append(self.connection(card, "byKey", dotted, target, {"cardinality": cardinality}))
            rows.append(row)
        return rows

    def connection(self, card, line, at, target, fields, extra=None):
        out = {"id": line_id(self.key, card, line, at, target or "", json.dumps(fields, sort_keys=True)),
               "line": line, "from": at}
        if target is not None:
            out["to"] = self.ids[target]
        for k, v in (extra or {}).items():
            out[k] = v
        if fields:
            out["fields"] = fields
        return out

    def card(self, spec, order, section):
        name = spec["name"]
        fields = {}
        if spec.get("label"):
            fields["label"] = spec["label"]
        if spec.get("d"):
            fields["description"] = spec["d"]
        tags = (spec.get("tags") or []) + (spec.get("props") or []) + self.card_tags.get(name, [])
        if tags:
            fields["tags"] = tags
        if spec.get("metadata"):
            fields["metadata"] = spec["metadata"]
        if spec.get("rules"):
            fields["rules"] = spec["rules"]
        if spec.get("absent"):
            fields["absentFields"] = spec["absent"]
        if spec.get("discriminator"):
            fields["discriminator"] = spec["discriminator"]
        out = {"id": self.ids[name], "kind": spec["kind"], "name": name, "section": section, "v": CARD_V}
        if order:
            out["order"] = order
        if fields:
            out["fields"] = fields
        connections = []
        if spec["kind"] == "enum":
            values = []
            for v, label in spec["values"]:
                body = {"value": v}
                if label:
                    body["label"] = label
                values.append({"id": self.id("value", name, v), "fields": body})
            out["rows"] = {"values": values}
        else:
            out["rows"] = {"fields": self.field_rows(name, spec["fields"], [], connections,
                                                     required_default=self.key != "small")}
        for line, at, target, extra in spec.get("lines", []):
            if line == "link":
                connections.append(self.connection(name, "link", at, target, None, extra))
            else:
                connections.append(self.connection(name, line, at, target, extra))
        if connections:
            out["connections"] = connections
        return out

    def files(self):
        """The project file of now and of before (version 18)."""
        main = self.id("section", "Main")
        schemas = self.id("section", "Schemas")
        sections = [
            {"id": main, "name": "Main", "created_at": self.created_at},
            {"id": schemas, "parent_id": main, "name": "Schemas", "created_at": self.created_at, "kind": "schema"},
            {"id": self.id("section", "Docs"), "parent_id": main, "name": "Docs", "sort_order": 1,
             "created_at": self.created_at, "kind": "document"},
            {"id": self.id("section", "Charts"), "parent_id": main, "name": "Charts", "sort_order": 2,
             "created_at": self.created_at, "kind": "chart"},
            {"id": self.id("section", "Free"), "parent_id": main, "name": "Free", "sort_order": 3,
             "created_at": self.created_at, "kind": "free"},
        ]
        workdirs = [
            {"id": f"dir:{dir_id}", "parent_id": main, "name": n, "sort_order": 4 + i,
             "created_at": self.created_at, "kind": "workdir"}
            for i, (dir_id, n) in enumerate(self.dirs)
        ]
        board_root = self.id("board", "Board")
        goals = self.id("board", "Goals")
        board = [
            {"id": board_root, "name": "Board", "created_at": self.created_at},
            {"id": goals, "parent_id": board_root, "name": "Goals", "created_at": self.created_at, "kind": "goals"},
        ]
        board_now = board + [
            {"id": self.id("board", "Complete"), "parent_id": goals, "name": "Complete",
             "created_at": self.created_at, "kind": "complete"},
            {"id": self.id("board", "Cancelled"), "parent_id": goals, "name": "Cancelled", "sort_order": 1,
             "created_at": self.created_at, "kind": "cancelled"},
        ]
        cards = [self.card(spec, i, schemas) for i, spec in enumerate(self.cards)]
        columns = 13
        places = sorted(
            ({"item_kind": "card", "item": c["id"], "x": 1 + (i % columns) * 14, "y": 1 + (i // columns) * 36}
             for i, c in enumerate(cards)),
            key=lambda p: p["item"],
        )
        main_sections = [{"type": "document", "name": "Docs"}, {"type": "schema", "name": "Schemas"},
                         {"type": "chart", "name": "Charts"}, {"type": "free", "name": "Free"}]
        tree_now = {"main_sections": main_sections + [
            {"type": "workdir", "name": n, "directory": dir_id} for dir_id, n in self.dirs],
            "board_sections": [{"type": "goals", "name": "Goals"}]}
        tree_before = {"main_sections": main_sections, "board_sections": [{"type": "goals", "name": "Goals"}]}

        def project(tree, exports):
            out = {"id": self.id("project"), "name": self.name, "slug": self.slug}
            if self.description:
                out["description"] = self.description
            out["created_at"] = self.created_at
            out["tag_groups"] = self.tag_groups
            out["export_dirs"] = [{"id": dir_id, "name": n} for dir_id, n in self.dirs]
            kinds = {}
            if self.line_kinds:
                kinds["lines"] = self.line_kinds
            kinds["project_tree"] = tree
            out["kinds"] = kinds
            if exports is not None:
                out["exports"] = exports
            return out

        targets = [("types.ts", None), ("types.zod.ts", "zod"), ("types.schema.json", None)]
        exports = []
        for (dir_id, n), (path, form) in zip(self.dirs, targets):
            entries = []
            for c in cards:
                entry = {"from": c["id"], "to": path}
                if form:
                    entry["format"] = form
                entries.append(entry)
            exports.append({"id": self.id("export", n), "name": n, "entries": entries})
        now = {
            "format": "specboard/project",
            "version": VERSION,
            "project": project(tree_now, exports),
            "sections": sections + workdirs,
            "cards": cards,
            "board": {"sections": board_now, "cards": []},
            "places": places,
        }
        settings = []
        for (dir_id, _), (path, form) in zip(self.dirs, targets):
            s = {"dir": dir_id, "path": path}
            if form:
                s["format"] = form
            settings.append(s)
        before_cards = [{**c, "exportTo": settings} for c in cards]
        before = {
            "format": "specboard/project",
            "version": VERSION_BEFORE,
            "project": project(tree_before, None),
            "sections": sections,
            "cards": before_cards,
            "board": {"sections": board, "cards": []},
            "places": places,
        }
        return now, before


LARGE = Project(
    "large",
    "Harbor City Library Network",
    "harbor-city-library-network",
    "港町市の図書館ネットワーク（中央館と分館、巡回車）でやりとりする記録の形。貸出・予約・館間の配送・受け入れ・延滞金・行事・監査の記録を、"
    "どの館でも同じ形で読み書きするために決める。欄の名前は送り合う JSON の綴りそのままの snake_case とし、画面の言葉は表示名に書く。"
    "作り物の見本で、実在の図書館とは関係しない。",
    "2026-01-15T09:00:00.000Z",
    LARGE_TAG_GROUPS,
    LARGE_ENUMS + LARGE_VALUES + LARGE_ENTITIES,
    line_kinds=[BYKEY_LINE],
)

DATAMODEL = Project(
    "datamodel",
    "データモデルの見本",
    "データモデルの見本",
    "図書館の貸し出しを例にした小さなデータモデル（作り物）。列挙 4 つと entity 5 つで、入れ子・自分を指す参照・履歴の記録を持つ。",
    "2026-01-15T09:00:00.000Z",
    [{"name": "分類", "exclusive": False, "tags": ["履歴", "中核", "語彙"]}],
    DATAMODEL_CARDS,
    card_tags=DATAMODEL_TAGS,
)

SMALL = Project(
    "small",
    "最小の見本",
    "最小の見本",
    None,
    "2026-01-15T09:00:00.000Z",
    [],
    SMALL_CARDS,
)


def text(value):
    return json.dumps(value, ensure_ascii=False, indent=2) + "\n"


def outputs():
    out = {}
    for project in (LARGE, DATAMODEL, SMALL):
        now, before = project.files()
        body = text(now)
        out[FIXTURES / "projects" / f"{project.key}.json"] = body
        out[SWIFT_FIXTURES / f"{project.key}.json"] = body
        if project is LARGE:
            out[FIXTURES / "projects" / "before" / f"{project.key}.json"] = text(before)
    return out


def stats(project):
    now, _ = project.files()
    kinds = {}
    fields = 0
    lines = {}

    def walk(rows):
        nonlocal fields
        for r in rows.get("fields", []):
            fields += 1
            if "rows" in r:
                walk(r["rows"])

    for c in now["cards"]:
        kinds[c["kind"]] = kinds.get(c["kind"], 0) + 1
        walk(c["rows"])
        for x in c.get("connections", []):
            lines[x["line"]] = lines.get(x["line"], 0) + 1
    return {"cards": len(now["cards"]), "kinds": kinds, "fields": fields, "lines": lines}


def main(argv):
    if "--stats" in argv:
        for project in (LARGE, DATAMODEL, SMALL):
            print(project.key, stats(project))
        return 0
    wanted = outputs()
    if "--check" in argv:
        stale = [str(p.relative_to(REPO)) for p, body in wanted.items()
                 if not p.exists() or p.read_text(encoding="utf-8") != body]
        for p in stale:
            print(f"out of date: {p}")
        return 1 if stale else 0
    for path, body in wanted.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")
        print(f"wrote {path.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
