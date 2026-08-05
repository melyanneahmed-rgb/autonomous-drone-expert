# M0 Foundations Gate

**الحالة:** معتمد (`APPROVED`) — بصيغته بعد تطبيق `docs/m0/amendment.md`
**التاريخ:** 2026-08-05
**المرجع الحاكم:** `docs/foundational/v1.1.md`

> هذه الوثيقة تعرض المحتوى **النافذ** لبوابة M0 بعد دمج التصحيحات الثمانية. سجل التغيير
> نفسه في `amendment.md`.

## 1. Repository and Workspace Isolation Plan

| البند | القرار النافذ |
| --- | --- |
| اسم المستودع | `autonomous-drone-expert` |
| المالك | `melyanneahmed-rgb` (النقل إلى منظمة مؤجل حتى وجود فريق أو كيان تجاري) |
| الرؤية | `Private` |
| الفرع الافتراضي | `main` |
| مساحة العمل | مستقلة تمامًا، مصدرها هذا المستودع وحده |

**قواعد حماية `main` المستهدفة:** كل التغييرات عبر Pull Request • منع force-push • منع حذف
`main` • require conversation resolution • linear history • required status checks بعد
ظهور أسماء الفحوص الفعلية • منع الدمج عند فشل الفحوص المطلوبة.

**قيود واعية على الحماية:** عدد الموافقات البشرية المطلوبة يبقى صفرًا ما دام لا يوجد مراجع
مستقل عن مؤلف الـ PR؛ `Require review from Code Owners` لا يُفعَّل لنفس السبب؛ لا تُفرض
signed commits قبل إثبات أن أداة التنفيذ تنشئ commits موقعة؛ ولا تُستخدم استثناءات صامتة أو
bypass.

**استراتيجية الفروع:** trunk-based بفروع قصيرة: `feat/*`، `fix/*`، `chore/*`، `docs/*`،
`spike/*` (لا تُدمج)، `provenance/*`. لا فروع طويلة العمر حتى M11.

**منع الخلط والاستيراد العرضي (مصحح — البند 4 في الملحق، ثم دفعة التصحيح التأسيسية):**
الفحوص تمنع **الاقتران البنيوي** لا ذكر الأسماء: منع submodules، منع `git`/`path`
dependencies خارج المستودع (بفحص احتواء مسار حقيقي لا مقارنة نصية)، منع remotes إضافية،
ومنع أسماء مجلدات vendored المعروفة. ذكر أسماء المشاريع في التوثيق **مسموح**.

**تصحيح صريح:** CI **لا يكتشف** الملفات المنسوخة أو المشتقة — لا يوجد فحص تشابه ولا مقارنة
بصمات. اكتشاف النسخ يعتمد على سياسة provenance والمراجعة البشرية، والادعاء بغير ذلك تطمين
زائف.

## 2. Technology Decision Record

| القرار | الحسم |
| --- | --- |
| إطار سطح المكتب | Tauri 2 — نهائي لـ M1–M7، يُعاد تقييمه عند M8 (بوابة DFU/drivers). **يُستخدم أحدث إصدار مستقر ومراجَع وقت الإنشاء ويُثبَّت بدقة** |
| Rust edition | 2024 |
| Toolchain | مثبّت بدقة في `rust-toolchain.toml` |
| MSRV | موثّق في `Cargo.toml` (`rust-version`)، منفصل عن التثبيت |
| UI | TypeScript صارم + React |
| مدير الحزم | pnpm (workspaces) |
| بنية المستودع | Monorepo: Cargo workspace + pnpm workspace |
| اختبارات | Rust: `cargo test` + proptest + cargo-fuzz + insta + nextest + llvm-cov. TS: Vitest + Playwright (لاحقًا) |
| التغليف | Tauri bundler، MSI/NSIS على Windows، التوقيع مؤجل لما قبل أي توزيع |
| Windows-first | CI على Windows منذ أول commit |
| macOS/Linux | كود خاص بالنظام محصور في `transport`؛ Linux في CI من M0؛ macOS من M5 بناءً وكامل عند M11 |

## 3. Dependency Audit

**لم تُدخل أي اعتمادية.** التثبيت النهائي يتم عبر `cargo-deny` لا بالثقة في جدول.

| الغرض | المرشح | الترخيص | القرار |
| --- | --- | --- | --- |
| Serial transport | `serialport` | MPL-2.0 (مؤكد) | **مرشح مشروط** — لا يُدمج قبل Windows spike يقارنه ببديل واحد على الأقل |
| بدائل Serial | `serial2`/`serial2-tokio`، `tokio-serial`+`mio-serial` | يُؤكد آليًا | مرشحات للمقارنة |
| Async/إلغاء/مهلات | `tokio` + `tokio-util` | يُؤكد آليًا | مرشح |
| USB enumeration/hotplug | `nusb` | Apache-2.0 OR MIT (مؤكد) | مرشح أساسي — Rust خالص بلا libusb |
| بديل USB | `rusb` | MIT | بديل — يجرّ libusb ومشاكل drivers |
| تخزين محلي آمن | كتابة ذرية + JSONL append-only | يُؤكد | مرشح — يخدم crash-safety |
| Checksums/توقيع | `sha2`/`blake3`، `ed25519-dalek`/`minisign-verify` | يُؤكد | مرشح — التوقيع من M10 |
| Logging/Audit | `tracing` (+ سجل تدقيق مستقل append-only) | يُؤكد | مرشح |
| Serialization | `serde`/`serde_json` | يُؤكد | مرشح |
| Property testing | `proptest` | يُؤكد | مرشح أساسي |
| Fuzzing | `cargo-fuzz`/`libfuzzer-sys`/`arbitrary` | يُؤكد | مرشح — يُشغَّل على Linux |
| Tauri | `tauri` + capabilities الرسمية | MIT/Apache-2.0 | مرشح أساسي |
| **مرفوض** | `tauri-plugin-serialplugin` وأي plugin serial طرف ثالث | — | **رفض**: يضع البروتوكول خارج سيطرتنا |

**معايير Windows spike الإلزامية:** enumeration (مع USB metadata: VID/PID/serial/
manufacturer) • open/close • cancellation • timeout • unplug/replug • reboot reconnect •
port busy / permission denied.

**DFU:** تدقيق منفصل قبل M8. الواجهة مثبتة مفاهيميًا: enumerate / open / read memory map /
erase / write / verify / leave، وكل عملية تُصرّح Recovery Class ولا تبدأ بلا target مؤكد
وbackup. مسار الاسترداد عبر bootloader الـ ROM يوصف بـ
**`Expected recovery path requiring hardware validation`** ولا يوصف بأنه مضمون.

## 4. Source Provenance Policy

المصادر المسموح بها بترتيب السلطة: (1) مواصفات ووثائق رسمية منشورة؛ (2) الكود المصدري
الرسمي **للقراءة والفهم فقط** لاستخراج حقائق واجهة توصف بكلماتنا؛ (3) ملاحظة سلوكية مباشرة
من عتاد أو Mock؛ (4) وثائق المصنّعين؛ (5) مصادر مجتمعية لاكتشاف وجود مشكلة فقط.

ممنوع: نسخ أو ترجمة آلية لأي كود أو تعليقات أو اختبارات أو fixtures أو جداول مولّدة أو بنية
داخلية أو رسائل خطأ. وممنوع استخدام أي كود من مشاريع المالك السابقة.

**قاعدة ملزمة:** كل مصدر يشير إلى **tag أو commit مثبّت** — لا فرع متحرك (`master`/`main`/
`HEAD`). التفاصيل والقالب في `provenance/README.md`.

**نموذج الحالة (ADR-0008):** لكل سجل بُعدان مستقلان — `source_state`
(`PINNED_SOURCE_RECORDED`) و`verification_state` (`NOT_REPRODUCED` → `MOCK_EXERCISED` →
`HARDWARE_OBSERVED`). الـ Mock يثبت اتساق تنفيذنا لا صحة المصدر الرسمي، ولا يُعلن دعم عتاد
قبل `HARDWARE_OBSERVED`. تخطيط الحمولة يجوز توثيقه في أي حالة تحقق ما دام من مصدر مثبّت.

**مراجعة طبقة البروتوكول:** كل PR يمس `crates/protocol-*` يحمل وسم `protocol-change`
ويستلزم سجلات provenance مطابقة، وحالة معلنة بصدق، وgolden packets من إنتاجنا لا من
fixtures المشاريع الرسمية.

**يحتاج مراجعة قانونية قبل التضمين:** target definitions • official presets • أي جدول مشتق
من مستودعات GPL • fixtures خارجية • أي مكتبة GPL/AGPL • crates التشفير • MPL-2.0 عند تعديل
ملفاتها • أي أصول خارجية.

## 5. Temporary Licensing Posture

المستودع Private • لا ملف `LICENSE` • `NOTICE.md` يعلن العمل الداخلي وحفظ الحقوق مؤقتًا
ومنع التوزيع وتأجيل الترخيص وأن هذا وضع هندسي لا رأي قانوني • موانع نشر تقنية في CI (يفشل
البناء عند ظهور `LICENSE` أو workflow نشر) • يُحسم الترخيص قبل أي توزيع وبعد اكتمال البنود
الخمسة في الوثيقة التأسيسية القسم 12.

## 6. First Target Selection

- **الأساسي:** SpeedyBee F405 V4 — target `SPEEDYBEEF405V4` — Betaflight 4.5.5.
- **الاحتياطي:** Matek F405-TE — target `MATEKF405TE` — Betaflight 4.5.5.
- الحالة: `PROPOSED — NOT PURCHASED OR HARDWARE VALIDATED`.
- بند مفتوح: تغذية خط 5V ومسار BZ+/BZ- عبر USB وحده **غير مثبتة**؛ لذلك التحقق السمعي
  اختياري ولا يدخل معايير قبول M1.

## 7. Beeper Vertical Slice Contract

العقد التفصيلي في `docs/adr/ADR-0006`. **توثيق فقط في هذه الدفعة.**

## 8. Initial Repository Structure

المنفذة في هذه الدفعة: workspace هيكلي بـ 16 crate، `app/` و`ui/` كتوثيق، `docs/`،
`provenance/`، `scripts/`، `.github/`.

## 9. CI and Quality Gates

المفعّل الآن (بصدق، بلا اعتماديات): `cargo fmt --check` • `cargo clippy -D warnings` •
`cargo test` • **فحص MSRV مستقل** (`cargo +1.85.0 check`) يثبت الفصل بين سلسلة أدوات
التطوير والـ MSRV المعلن • فحص `#![forbid(unsafe_code)]` في كل crate • منع `.gitmodules` •
منع `git`/`path` dependencies خارجية • منع workflow نشر • منع ملف `LICENSE` • تحقق بنية
سجلات provenance • فحص أن مصادر Betaflight تستخدم tag/commit مثبتًا لا `master` • فحص أنماط
أسرار محلي • **اختبارات انحدار لسكربتات البوابات نفسها** • Windows CI • Linux CI.

**مؤجل إلى دفعات لاحقة مع مكوناته:** Tauri build • React build • Vitest • Playwright •
cargo-fuzz • property tests • Serial/USB/DFU tests • Beeper tests • hardware tests •
release workflow • public artifacts • تفعيل `cargo-deny` و`cargo-audit` كفحوص مطلوبة.

## 10. M0 Go/No-Go

**التوصية النافذة:** `FINAL GO — FOUNDATION REPOSITORY ONLY`.

- البرمجة الإنتاجية: **غير معتمدة**.
- الكتابة على العتاد: **غير معتمدة**.
- تنفيذ شريحة Beeper: **غير معتمد في هذه الدفعة**.
- تعديل أي مشروع قائم: **محظور**.
