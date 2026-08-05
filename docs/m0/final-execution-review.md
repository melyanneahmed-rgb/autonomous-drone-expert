# M0 Final Execution Review

**الحالة:** معتمد (`APPROVED`)
**القرار الصادر:** `FINAL GO — FOUNDATION REPOSITORY ONLY`
**التاريخ:** 2026-08-05

## 1. ما تغطيه هذه الموافقة

إنشاء **مستودع أساس محمي** فقط: هيكل، وثائق، سياسات، وبوابات جودة. لا منطق إنتاجي، ولا
اعتماديات، ولا واجهة عاملة، ولا اتصال بعتاد.

## 2. ما لا تغطيه (محظورات سارية)

- تنفيذ MSP codec أو CLI، أو إدخال أي Command ID إلى كود Rust.
- إضافة أي اعتمادية Serial أو USB أو DFU، أو تشغيل Windows Serial Spike.
- إنشاء تطبيق Tauri فعلي أو تثبيت React أو أي runtime dependency.
- تنفيذ شريحة Beeper.
- توصيل أو اختبار أو الكتابة على أي Flight Controller، أو شراء عتاد.
- تنزيل firmware أو إنشاء Firmware Capability Pack إنتاجية.
- نسخ كود من Betaflight أو INAV أو من أي مشروع سابق للمالك.
- تعديل أي مشروع قائم.
- دمج الـ Draft PR، أو نشر أي source أو binary أو artifact عام، أو إنشاء release.
- استخدام force-push أو reset أو clean أو rebase أو amend.

## 3. التسلسل التنفيذي المعتمد

1. Bootstrap Commit واحد على `main` (`README.md`، `NOTICE.md`، `.gitignore` فقط).
2. فرع `chore/m0-foundation` من Bootstrap SHA.
3. Foundation Commit واحد مقصود.
4. دفع الفرع **دون** force-push.
5. فتح **Draft** Pull Request إلى `main`.
6. تشغيل CI.
7. جعل الفحوص الناجحة مطلوبة على `main` عند توفر أسماء الفحوص وسماح الإعدادات.
8. **التوقف قبل الدمج** — ولا تحويل من Draft إلى Ready دون موافقة صريحة.

## 4. الاستثناء الوحيد الموثق

Bootstrap Commit هو **الدفع المباشر الوحيد المسموح به إلى `main`** في عمر المشروع، وقد
اقتضته ضرورة وجود `main` وتاريخ مشترك حتى يمكن فتح فرع وPull Request أصلًا. لا يتكرر.

## 5. البوابة التالية

لا تبدأ أي برمجة إنتاجية ولا Windows Serial Spike ولا شريحة Beeper قبل مراجعة المالك
للمستودع وموافقته الصريحة على الدفعة التالية.
