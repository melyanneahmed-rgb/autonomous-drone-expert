import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const read = (relative) => fs.readFileSync(new URL(`../${relative}`, import.meta.url), "utf8");

test("approved Arabic RTL identity and critical copy are preserved", () => {
  const html = read("index.html");
  const app = read("src/App.tsx");
  const styles = read("src/styles.css");

  assert.match(html, /<html lang="ar" dir="rtl">/);
  for (const text of [
    "Smart Configurator",
    "خبير الدرون المستقل",
    "بياناتك تبقى على جهازك",
    "عرّف درونك.",
    "واترك الباقي للخبير.",
    "هذه نسخة واجهة قبل الاعتماد العتادي",
  ]) {
    assert.ok(`${html}\n${app}`.includes(text), `missing approved text: ${text}`);
  }
  assert.match(styles, /--ink:\s*#10231f/);
  assert.doesNotMatch(styles, /tailwindcss/i);
  assert.doesNotMatch(`${html}\n${app}\n${styles}`, /https?:\/\//i);
});

test("UI source contains no browser device API", () => {
  const source = `${read("src/App.tsx")}\n${read("src/main.tsx")}`;
  assert.doesNotMatch(source, /navigator\s*\.\s*(serial|usb|hid|bluetooth)/i);
  assert.doesNotMatch(source, /\b(SerialPort|USBDevice|HIDDevice|BluetoothDevice|requestPort|requestDevice)\b/);
});
