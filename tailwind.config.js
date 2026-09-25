/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: "var(--ink)",
        bg: "var(--bg)",
        surface: "var(--surface)",
        raised: "var(--raised)",
        line: "var(--line)",
        body: "var(--text)",
        muted: "var(--muted)",
        accent: "var(--accent)",
        ok: "var(--ok)",
        warn: "var(--warn)",
        err: "var(--err)",
        info: "var(--info)",
      },
      fontFamily: {
        sans: ["Segoe UI Variable Text", "Segoe UI", "system-ui", "sans-serif"],
        mono: ["Cascadia Code", "Cascadia Mono", "Consolas", "ui-monospace", "monospace"],
      },
      borderRadius: { panel: "4px" },
    },
  },
  plugins: [],
};
