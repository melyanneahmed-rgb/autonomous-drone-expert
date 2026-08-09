import { useMemo, useState } from "react";
import type { ChangeEvent } from "react";

type SetupProfile = "cinematic" | "freestyle" | "racing" | "long-range";
type FirmwareSource = "online" | "manual";
type ControlFunction = "arm" | "mode" | "buzzer" | "rescue" | "turtle";

type DroneSpec = {
  frame: string;
  battery: string;
  motor: string;
  motorKv: string;
  propeller: string;
  esc: string;
  radio: string;
  video: string;
  gps: string;
};

const initialSpec: DroneSpec = {
  frame: "5 بوصات",
  battery: "6S",
  motor: "2207",
  motorKv: "1950 KV",
  propeller: "5.1 × 3.6 × 3",
  esc: "55A 4-in-1",
  radio: "ExpressLRS 2.4 GHz",
  video: "رقمي",
  gps: "موجود",
};

const profiles: Array<{
  id: SetupProfile;
  title: string;
  english: string;
  description: string;
  accent: string;
}> = [
  {
    id: "cinematic",
    title: "سينمائي",
    english: "Cinematic",
    description: "حركة ناعمة، ثبات للصورة، واستجابة متوقعة.",
    accent: "هدوء",
  },
  {
    id: "freestyle",
    title: "فريستايل",
    english: "Freestyle",
    description: "تحكم دقيق مع قوة متوازنة للمناورات.",
    accent: "توازن",
  },
  {
    id: "racing",
    title: "سباق",
    english: "Racing",
    description: "استجابة حادة، وزن أقل، وأقل زمن تأخير.",
    accent: "سرعة",
  },
  {
    id: "long-range",
    title: "مدى بعيد",
    english: "Long range",
    description: "كفاءة، حماية للطاقة، ومسارات إنقاذ محسوبة.",
    accent: "كفاءة",
  },
];

const controlFunctions: Array<{ id: ControlFunction; label: string }> = [
  { id: "arm", label: "تسليح الدرون" },
  { id: "mode", label: "تغيير نمط الطيران" },
  { id: "buzzer", label: "تشغيل الصفارة" },
  { id: "rescue", label: "الإنقاذ الآمن" },
  { id: "turtle", label: "قلب الدرون" },
];

const planGroups = [
  {
    number: "01",
    title: "الطاقة والمحركات",
    detail: "حدود البطارية، تيار المحركات، بروتوكول ESC، واتجاه الدوران.",
    automatic: true,
  },
  {
    number: "02",
    title: "الراديو والتحكم",
    detail: "منفذ المستقبل، البروتوكول، ترتيب القنوات والـfailsafe.",
    automatic: true,
  },
  {
    number: "03",
    title: "أزرار التحكم",
    detail: "وظائف الأزرار والمفاتيح كما اخترتها أنت في الخطوة اليدوية.",
    automatic: false,
  },
  {
    number: "04",
    title: "أداء الطيران",
    detail: "المعدلات، الفلاتر، التحكم، الخمول وتعويض الجهد.",
    automatic: true,
  },
  {
    number: "05",
    title: "السلامة والاستعادة",
    detail: "النسخة الاحتياطية، تنبيهات الطاقة، الإنقاذ والتحقق بعد التشغيل.",
    automatic: true,
  },
  {
    number: "06",
    title: "الواجهة والملحقات",
    detail: "عرض البيانات، الفيديو، GPS، الصفارة وإشعارات حالة الدرون.",
    automatic: true,
  },
];

const steps = [
  "المكوّنات",
  "أسلوب الطيران",
  "أزرار التحكم",
  "الفريموير",
  "الخطة",
];

function Field({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: string[];
  onChange: (value: string) => void;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      <select value={value} onChange={(event) => onChange(event.target.value)}>
        {options.map((option) => (
          <option key={option}>{option}</option>
        ))}
      </select>
    </label>
  );
}

export default function App() {
  const [step, setStep] = useState(0);
  const [spec, setSpec] = useState<DroneSpec>(initialSpec);
  const [profile, setProfile] = useState<SetupProfile>("freestyle");
  const [firmwareSource, setFirmwareSource] =
    useState<FirmwareSource>("online");
  const [firmwareFile, setFirmwareFile] = useState("");
  const [firmwareHash, setFirmwareHash] = useState("");
  const [connection, setConnection] = useState<"idle" | "deferred">("idle");
  const [extraGoals, setExtraGoals] = useState<string[]>([
    "حماية المحركات",
    "إنقاذ GPS",
  ]);
  const [controlAssignments, setControlAssignments] = useState<
    Record<"switch1" | "switch2" | "button1", ControlFunction>
  >({
    switch1: "arm",
    switch2: "mode",
    button1: "buzzer",
  });

  const chosenProfile = profiles.find((item) => item.id === profile)!;
  const filledFields = Object.values(spec).filter(Boolean).length;
  const completeness = Math.round((filledFields / Object.keys(spec).length) * 100);
  const selectedControlFunctions = Object.values(controlAssignments);
  const hasControlConflict =
    new Set(selectedControlFunctions).size !== selectedControlFunctions.length;

  const summary = useMemo(() => {
    if (profile === "cinematic") {
      return "إعداد سلس بثبات أعلى للصورة واستجابة مدروسة حول المنتصف.";
    }
    if (profile === "racing") {
      return "إعداد سريع بزمن استجابة منخفض وحدود أمان مناسبة للبنية.";
    }
    if (profile === "long-range") {
      return "إعداد كفؤ للطاقة مع أولوية للرابط والإنقاذ والعودة الآمنة.";
    }
    return "إعداد متوازن للمناورات مع تحكم دقيق وحماية حرارية محسوبة.";
  }, [profile]);

  function updateSpec(key: keyof DroneSpec, value: string) {
    setSpec((current) => ({ ...current, [key]: value }));
  }

  function toggleGoal(goal: string) {
    setExtraGoals((current) =>
      current.includes(goal)
        ? current.filter((item) => item !== goal)
        : [...current, goal],
    );
  }

  function updateControlAssignment(
    input: keyof typeof controlAssignments,
    value: ControlFunction,
  ) {
    setControlAssignments((current) => ({ ...current, [input]: value }));
  }

  async function chooseFirmware(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) return;
    setFirmwareFile(file.name);
    setFirmwareHash("جارٍ حساب البصمة…");
    const digest = await crypto.subtle.digest("SHA-256", await file.arrayBuffer());
    const hex = Array.from(new Uint8Array(digest))
      .map((byte) => byte.toString(16).padStart(2, "0"))
      .join("");
    setFirmwareHash(`${hex.slice(0, 12)}…${hex.slice(-8)}`);
  }

  function showConnectionDeferred() {
    setConnection("deferred");
  }

  const connectionCopy = {
    idle: "لم يتم اختيار منفذ بعد",
    deferred: "اختيار USB مؤجل — لم يتصل التطبيق بأي جهاز",
  }[connection];

  return (
    <main className="app-shell">
      <header className="topbar">
        <a className="brand" href="#top" aria-label="الصفحة الرئيسية">
          <span className="brand-mark" aria-hidden="true">
            <i />
          </span>
          <span>
            <strong>Smart Configurator</strong>
            <small>خبير الدرون المستقل</small>
          </span>
        </a>

        <div className="topbar-status">
          <span className="privacy-pill">
            <i /> بياناتك تبقى على جهازك
          </span>
          <button className="language-button" type="button" aria-label="اللغة">
            AR
          </button>
        </div>
      </header>

      <section className="hero" id="top">
        <div className="hero-copy">
          <p className="eyebrow">إعداد ذكي · كامل · دون إنترنت</p>
          <h1>
            عرّف درونك.
            <br />
            <span>واترك الباقي للخبير.</span>
          </h1>
          <p className="hero-lead">
            أدخل مكوناتك واختر أسلوب الطيران. سنبني إعدادًا متوافقًا، نحمي
            النسخة الحالية، ثم نبرمج ونتحقق خطوة بخطوة.
          </p>
        </div>

        <div className="hero-visual" aria-label="تصور الدرون وحالة الجاهزية">
          <div className="orbit orbit-one" />
          <div className="orbit orbit-two" />
          <div className="drone-core">
            <div className="core-dot" />
            <span>جاهز للتحليل</span>
          </div>
          <span className="rotor rotor-a" />
          <span className="rotor rotor-b" />
          <span className="rotor rotor-c" />
          <span className="rotor rotor-d" />
          <div className="visual-tag tag-a">9 مكوّنات</div>
          <div className="visual-tag tag-b">فحص محلي</div>
        </div>
      </section>

      <section className="workspace" aria-label="مسار إعداد الدرون">
        <nav className="stepper" aria-label="مراحل الإعداد">
          {steps.map((label, index) => (
            <button
              className={index === step ? "active" : index < step ? "done" : ""}
              key={label}
              onClick={() => setStep(index)}
              type="button"
            >
              <span>{index < step ? "✓" : index + 1}</span>
              <b>{label}</b>
            </button>
          ))}
        </nav>

        <div className="workspace-grid">
          <div className="main-panel">
            {step === 0 && (
              <section className="step-content">
                <div className="section-heading">
                  <div>
                    <p>الخطوة 1 من 5</p>
                    <h2>ما الذي بُني منه درونك؟</h2>
                  </div>
                  <span className="completion">{completeness}% مكتمل</span>
                </div>
                <p className="section-intro">
                  لا تحتاج إلى معرفة الإعدادات المعقدة. أدخل ما هو مكتوب على
                  القطع وسنتولى مطابقة الحدود والقيم المناسبة.
                </p>

                <div className="fields-grid">
                  <Field
                    label="حجم الإطار"
                    value={spec.frame}
                    options={["2.5 بوصة", "3 بوصات", "3.5 بوصة", "5 بوصات", "6 بوصات", "7 بوصات"]}
                    onChange={(value) => updateSpec("frame", value)}
                  />
                  <Field
                    label="البطارية"
                    value={spec.battery}
                    options={["2S", "3S", "4S", "5S", "6S", "8S"]}
                    onChange={(value) => updateSpec("battery", value)}
                  />
                  <Field
                    label="حجم المحرك"
                    value={spec.motor}
                    options={["1103", "1404", "2004", "2207", "2306", "2806.5"]}
                    onChange={(value) => updateSpec("motor", value)}
                  />
                  <Field
                    label="قيمة KV للمحرك"
                    value={spec.motorKv}
                    options={["1200 KV", "1750 KV", "1950 KV", "2450 KV", "3800 KV", "5000 KV"]}
                    onChange={(value) => updateSpec("motorKv", value)}
                  />
                  <Field
                    label="المراوح"
                    value={spec.propeller}
                    options={["3 × 3 × 3", "3.5 × 2.8 × 3", "5.1 × 3.6 × 3", "5.1 × 4.3 × 3", "7 × 3.5 × 3"]}
                    onChange={(value) => updateSpec("propeller", value)}
                  />
                  <Field
                    label="متحكم السرعة ESC"
                    value={spec.esc}
                    options={["20A 4-in-1", "35A 4-in-1", "45A 4-in-1", "55A 4-in-1", "65A 4-in-1"]}
                    onChange={(value) => updateSpec("esc", value)}
                  />
                  <Field
                    label="بروتوكول رابط الراديو"
                    value={spec.radio}
                    options={["ExpressLRS 2.4 GHz", "ExpressLRS 900 MHz", "Crossfire", "Ghost", "SBUS", "IBUS", "DSMX", "غير متأكد"]}
                    onChange={(value) => updateSpec("radio", value)}
                  />
                  <Field
                    label="نظام الفيديو"
                    value={spec.video}
                    options={["رقمي", "تناظري", "لا يوجد"]}
                    onChange={(value) => updateSpec("video", value)}
                  />
                  <Field
                    label="نظام تحديد الموقع GPS"
                    value={spec.gps}
                    options={["موجود", "غير موجود", "سأضيفه لاحقًا"]}
                    onChange={(value) => updateSpec("gps", value)}
                  />
                </div>

                <div className="smart-note">
                  <span className="note-icon">✦</span>
                  <div>
                    <strong>لن نسمح بتركيبة غير آمنة</strong>
                    <p>
                      سيقارن التطبيق الجهد، KV، حجم المراوح، تحمل ESC وبقية
                      القطع قبل إنشاء أي إعداد.
                    </p>
                  </div>
                </div>
              </section>
            )}

            {step === 1 && (
              <section className="step-content">
                <div className="section-heading">
                  <div>
                    <p>الخطوة 2 من 5</p>
                    <h2>كيف تريد أن يطير؟</h2>
                  </div>
                  <span className="completion">قرار واحد واضح</span>
                </div>
                <p className="section-intro">
                  اختر إحساس الطيران. التطبيق يحوّل هذا الاختيار إلى كل
                  القيم التقنية المطلوبة دون أن يطلب منك ضبطها يدويًا.
                </p>

                <div className="profile-grid">
                  {profiles.map((item) => (
                    <button
                      className={`profile-card ${profile === item.id ? "selected" : ""}`}
                      key={item.id}
                      onClick={() => setProfile(item.id)}
                      type="button"
                    >
                      <span className="profile-check">
                        {profile === item.id ? "✓" : ""}
                      </span>
                      <small>{item.english}</small>
                      <h3>{item.title}</h3>
                      <p>{item.description}</p>
                      <b>{item.accent}</b>
                    </button>
                  ))}
                </div>

                <div className="goal-box">
                  <div>
                    <p>أهداف إضافية</p>
                    <h3>سنضيفها إلى الخطة تلقائيًا</h3>
                  </div>
                  <div className="goal-list">
                    {["حماية المحركات", "إنقاذ GPS", "أطول زمن طيران", "أقل ضوضاء للفيديو"].map((goal) => (
                      <button
                        className={extraGoals.includes(goal) ? "selected" : ""}
                        key={goal}
                        onClick={() => toggleGoal(goal)}
                        type="button"
                      >
                        <span>{extraGoals.includes(goal) ? "✓" : "+"}</span>
                        {goal}
                      </button>
                    ))}
                  </div>
                </div>
              </section>
            )}

            {step === 2 && (
              <section className="step-content">
                <div className="section-heading">
                  <div>
                    <p>الخطوة 3 من 5</p>
                    <h2>عيّن أزرار التحكم بنفسك</h2>
                  </div>
                  <span className="completion manual">قرارك أنت</span>
                </div>
                <p className="section-intro">
                  التطبيق يقرأ حركة كل مفتاح ويتحقق من التعارض، لكنه لا يقرر
                  وظيفة زر بدلًا عنك. في الاتصال الحقيقي سنطلب منك تحريك كل
                  زر، ثم تحفظ الوظيفة التي تريدها.
                </p>

                <div className="control-assignment-list">
                  <label>
                    <span className="control-input-name">
                      <i /> المفتاح المكتشف 1
                    </span>
                    <select
                      value={controlAssignments.switch1}
                      onChange={(event) =>
                        updateControlAssignment(
                          "switch1",
                          event.target.value as ControlFunction,
                        )
                      }
                    >
                      {controlFunctions.map((option) => (
                        <option key={option.id} value={option.id}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                    <b>اختيار يدوي</b>
                  </label>
                  <label>
                    <span className="control-input-name">
                      <i /> المفتاح المكتشف 2
                    </span>
                    <select
                      value={controlAssignments.switch2}
                      onChange={(event) =>
                        updateControlAssignment(
                          "switch2",
                          event.target.value as ControlFunction,
                        )
                      }
                    >
                      {controlFunctions.map((option) => (
                        <option key={option.id} value={option.id}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                    <b>اختيار يدوي</b>
                  </label>
                  <label>
                    <span className="control-input-name">
                      <i /> الزر المكتشف 1
                    </span>
                    <select
                      value={controlAssignments.button1}
                      onChange={(event) =>
                        updateControlAssignment(
                          "button1",
                          event.target.value as ControlFunction,
                        )
                      }
                    >
                      {controlFunctions.map((option) => (
                        <option key={option.id} value={option.id}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                    <b>اختيار يدوي</b>
                  </label>
                </div>

                <div className="smart-note manual-note">
                  <span className="note-icon">✓</span>
                  <div>
                    <strong>بقية إعدادات الراديو تلقائية</strong>
                    <p>
                      المنفذ والبروتوكول وترتيب القنوات والـfailsafe يحددها
                      البرنامج ويطبقها ويتحقق منها؛ أنت تختار وظائف الأزرار فقط.
                    </p>
                  </div>
                </div>

                {hasControlConflict && (
                  <div className="control-conflict" role="alert">
                    اختر وظيفة مختلفة لكل زر أو مفتاح قبل المتابعة.
                  </div>
                )}
              </section>
            )}

            {step === 3 && (
              <section className="step-content">
                <div className="section-heading">
                  <div>
                    <p>الخطوة 4 من 5</p>
                    <h2>كيف تريد تجهيز الفريموير؟</h2>
                  </div>
                  <span className="completion">خياران آمنان</span>
                </div>
                <p className="section-intro">
                  يحدد التطبيق الحزمة المتوافقة مع وحدتك. يمكنك تنزيلها من
                  المصدر الموثوق أو اختيار ملف موجود على جهازك والعمل دون إنترنت.
                </p>

                <div className="source-grid">
                  <button
                    className={`source-card ${firmwareSource === "online" ? "selected" : ""}`}
                    onClick={() => setFirmwareSource("online")}
                    type="button"
                  >
                    <span className="source-icon online-icon">↓</span>
                    <div>
                      <small>موصى به</small>
                      <h3>تنزيل تلقائي موثوق</h3>
                      <p>
                        نختار الإصدار المتوافق، نتحقق من مصدره وبصمته، ثم
                        نحفظه محليًا قبل الفلاش.
                      </p>
                    </div>
                    <span className="radio-dot" />
                  </button>

                  <button
                    className={`source-card ${firmwareSource === "manual" ? "selected" : ""}`}
                    onClick={() => setFirmwareSource("manual")}
                    type="button"
                  >
                    <span className="source-icon">⌁</span>
                    <div>
                      <small>دون إنترنت</small>
                      <h3>اختيار ملف يدويًا</h3>
                      <p>
                        اختر ملف الفريموير من جهازك. لن يُرفع إلى أي خادم،
                        وسنفحص التوافق والبصمة محليًا.
                      </p>
                    </div>
                    <span className="radio-dot" />
                  </button>
                </div>

                {firmwareSource === "online" ? (
                  <div className="firmware-result">
                    <div className="firmware-badge">✓</div>
                    <div>
                      <small>الاختيار الذكي</small>
                      <h3>الحزمة المتوافقة ستُحدد بعد قراءة وحدة التحكم</h3>
                      <p>
                        لن تُعرض أسماء المنصات الداخلية في المسار العادي؛ يرى
                        المستخدم فقط التوافق، الإصدار، تاريخ الحزمة وحالة التحقق.
                      </p>
                    </div>
                    <span className="verified-label">تحقق مزدوج</span>
                  </div>
                ) : (
                  <label className="file-drop">
                    <input
                      accept=".hex,.bin,.dfu"
                      onChange={chooseFirmware}
                      type="file"
                    />
                    <span className="file-plus">+</span>
                    <div>
                      <strong>{firmwareFile || "اختر ملف الفريموير"}</strong>
                      <p>
                        {firmwareHash
                          ? `بصمة SHA-256: ${firmwareHash}`
                          : "HEX أو BIN أو DFU · تتم المعالجة داخل جهازك"}
                      </p>
                    </div>
                    <b>تصفح الملفات</b>
                  </label>
                )}

                <div className="flash-gate">
                  <div className="gate-lock" aria-hidden="true">◆</div>
                  <div>
                    <strong>الفلاش عملية مستقلة ومحميّة</strong>
                    <p>
                      لن يبدأ بعد التنزيل مباشرة. أولًا: توافق، نسخة احتياطية،
                      طاقة آمنة، خطة استعادة، ثم موافقة صريحة منك.
                    </p>
                  </div>
                </div>
              </section>
            )}

            {step === 4 && (
              <section className="step-content">
                <div className="section-heading">
                  <div>
                    <p>الخطوة 5 من 5</p>
                    <h2>خطة الإعداد الكاملة</h2>
                  </div>
                  <span className="completion ready">جاهزة للمراجعة</span>
                </div>

                <div className="plan-hero">
                  <div>
                    <span>إعداد {chosenProfile.title}</span>
                    <h3>{spec.frame} · {spec.battery} · {spec.motorKv}</h3>
                    <p>{summary}</p>
                  </div>
                  <div className="plan-score">
                    <strong>5 + 1</strong>
                    <small>آلي + اختيارك</small>
                  </div>
                </div>

                <div className="plan-list">
                  {planGroups.map((group) => (
                    <article key={group.number}>
                      <span>{group.number}</span>
                      <div>
                        <h3>{group.title}</h3>
                        <p>{group.detail}</p>
                      </div>
                      <b className={group.automatic ? "" : "manual-label"}>
                        {group.automatic ? "سيُضبط تلقائيًا" : "اختيارك اليدوي"}
                      </b>
                    </article>
                  ))}
                </div>

                <div className="beta-disclaimer">
                  <strong>هذه نسخة واجهة قبل الاعتماد العتادي</strong>
                  <p>
                    تعرض مسار المنتج الحقيقي، لكنها لا تنفّذ فلاشًا أو كتابة
                    على الدرون حاليًا. تُفتح تلك القدرة فقط بعد اختبارات M2
                    على عتاد حقيقي وبموافقتك.
                  </p>
                </div>
              </section>
            )}

            <footer className="panel-actions">
              <button
                className="secondary-button"
                disabled={step === 0}
                onClick={() => setStep((current) => Math.max(0, current - 1))}
                type="button"
              >
                السابق
              </button>
              <button
                className="primary-button"
                disabled={step === 2 && hasControlConflict}
                onClick={() => {
                  if (step < 4) setStep((current) => current + 1);
                  else showConnectionDeferred();
                }}
                type="button"
              >
                {step === 0 && "تحليل المكوّنات"}
                {step === 1 && "اعتماد أسلوب الطيران"}
                {step === 2 && "حفظ تعيين الأزرار"}
                {step === 3 && "بناء خطة الإعداد"}
                {step === 4 && "اختيار منفذ USB"}
                <span>←</span>
              </button>
            </footer>
          </div>

          <aside className="side-panel">
            <div className="side-head">
              <div>
                <p>ملخص درونك</p>
                <h2>Build 01</h2>
              </div>
              <span>{completeness}%</span>
            </div>

            <dl className="spec-summary">
              <div>
                <dt>الإطار</dt>
                <dd>{spec.frame}</dd>
              </div>
              <div>
                <dt>الطاقة</dt>
                <dd>{spec.battery} · {spec.esc}</dd>
              </div>
              <div>
                <dt>المحركات</dt>
                <dd>{spec.motor} · {spec.motorKv}</dd>
              </div>
              <div>
                <dt>الراديو</dt>
                <dd>{spec.radio}</dd>
              </div>
              <div>
                <dt>الطيران</dt>
                <dd>{chosenProfile.title}</dd>
              </div>
            </dl>

            <div className="connection-card">
              <div className="usb-symbol" aria-hidden="true">
                <span />
              </div>
              <p>اتصال وحدة التحكم</p>
              <strong aria-live="polite">{connectionCopy}</strong>
              <button
                onClick={showConnectionDeferred}
                type="button"
              >
                اختيار USB
              </button>
              <small>
                اختيار المنفذ لا يرسل أوامر ولا يكتب على الدرون في هذه النسخة.
              </small>
            </div>

            <div className="local-promise">
              <span className="pulse-dot" />
              <div>
                <strong>Offline-first</strong>
                <p>الإعدادات والنسخ الاحتياطية تبقى محليًا.</p>
              </div>
            </div>
          </aside>
        </div>
      </section>

      <footer className="site-footer">
        <p>Smart Configurator · قرار بسيط منك، إعداد خبير للدرون.</p>
        <span>نسخة تجربة المنتج · لا دعم عتادي مُعتمد بعد</span>
      </footer>
    </main>
  );
}
