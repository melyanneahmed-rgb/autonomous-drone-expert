# M0 Amendment — سجل التصحيحات

**الحالة:** معتمد (`APPROVED`) — صدر بعد قرار `CONDITIONAL GO`
**التاريخ:** 2026-08-05
**الأثر:** التصحيحات الثمانية أدناه **مدمجة** في `foundations-gate.md`. هذه الوثيقة سجل
التغيير لا نسخة موازية.

| # | التصحيح | الأثر |
| --- | --- | --- |
| 1 | حذف وصف Tauri 2.10.1 بأنه الأحدث | يُستخدم أحدث إصدار مستقر ومراجَع **وقت إنشاء المستودع**، يُدقَّق ثم يُثبَّت بدقة في lockfiles وtoolchain. القرار المعماري (Tauri 2) لم يتغير |
| 2 | provenance من tag لا من فرع | كل سجلات شريحة Betaflight 4.5.5 تُستخرج من **tag `4.5.5`**. أُلغيت السجلات المستخرجة من `master` كمصدر، وأُعيد الاستخراج. تخطيطات الحمولة ودلالة قناع الـ Beeper **لم تُوثَّق بعد** وتبقى `UNVERIFIED`. قاعدة ملزمة: لا provenance من فرع متحرك |
| 3 | لغة استرداد DFU | حُذفت عبارة "استرداد مضمون"؛ الصيغة المعتمدة: `Expected recovery path requiring hardware validation` |
| 4 | إعادة تصميم فحص العزل | أُلغي الفحص الذي يفشل لمجرد ذكر أسماء المشاريع السابقة (كان يمنع التوثيق المشروع). البديل يمنع الاقتران الفعلي: imports، paths، submodules، remotes، الملفات المنسوخة |
| 5 | تصنيف الدخول إلى CLI | لم يعد Recovery Class. التصنيف المعتمد: `SESSION_STATE_TRANSITION — NO PERSISTENT CONFIGURATION CHANGE`، مع توثيق طريقة الخروج وإعادة مزامنة MSP |
| 6 | معيار نجاح M1 | التحقق الإلكتروني لقناع الـ Beeper هو المعيار الوحيد. التحقق السمعي اختياري حتى يثبت على اللوحة أن BZ+/BZ- والجرس يعملان بطاقة USB وحدها |
| 7 | `serialport` مرشح مشروط | لا يُدمج قبل Windows spike يقارنه ببديل واحد على الأقل في: enumeration، open/close، cancellation، timeout، unplug/replug، reboot reconnect، port busy، وUSB metadata |
| 8 | الملكية | المستودع Private باسم `autonomous-drone-expert`، المالك المبدئي `melyanneahmed-rgb`، النقل إلى منظمة مؤجل، والعمل حصريًا في مساحة عمل جديدة مستقلة |

## بنود مضافة إلى معايير اكتمال M0

- (ح) كل سجلات provenance مرجعها tag `4.5.5` وليس فرعًا.
- (ط) تقرير Windows spike مكتمل وقرار crate الـ serial متخذ في PR مستقل قبل أي كود transport
  إنتاجي.
- (ي) إصدارات Tauri وسلسلة الأدوات مثبّتة ومدقّقة في lockfiles.

## تصحيح واقعي مسجل

الاسم `MSP_SET_REBOOT` المستخدم في تقارير سابقة **غير موجود** في مصدر Betaflight. الاسم
الصحيح `MSP_REBOOT`. مسجل في `provenance/records/`.
