import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { call, describe, ACTIVATION_PURCHASE_URL, AppError, SessionUser } from "../lib/api";

export default function Register({
  onRegistered,
}: { onRegistered: (u: SessionUser) => void; theme: string }) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [activationKey, setActivationKey] = useState("");
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [redirecting, setRedirecting] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    setRedirecting(false);
    try {
      const user = await call<SessionUser>("register", { username, password, activationKey });
      onRegistered(user);
    } catch (err) {
      const desc = describe(err);
      setError(desc);
      if (desc.code === "ACTIVATION_INVALID") {
        setRedirecting(true);
        openUrl(ACTIVATION_PURCHASE_URL).catch(() => undefined);
      }
      setPassword("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="h-full flex">
      {/* Left: the bench readout, so the first screen looks like the product. */}
      <div className="hidden md:flex flex-col justify-between w-[42%] bg-ink border-r border-line p-10">
        <div className="flex items-center gap-2.5">
          <div className="w-[22px] h-[3px] rounded-sm" style={{ background: "var(--accent)" }} />
          <span className="text-[15px] font-semibold tracking-tight">DevWorkstation</span>
        </div>

        <div className="mono text-[12px] leading-[1.9] text-muted">
          <div><span style={{ color: "var(--ok)" }}>●</span> CV builder, PDF studio, code analyzer — offline</div>
          <div><span style={{ color: "var(--ok)" }}>●</span> Database lab, test lab, terminal — offline</div>
          <div><span style={{ color: "var(--info)" }}>●</span> Website lab, servers — connect when needed</div>
          <div><span style={{ color: "var(--accent)" }}>●</span> Credentials — Windows Credential Manager</div>
        </div>

        <p className="text-[12px] text-muted max-w-[38ch]">
          Everything is stored on this computer. Nothing is uploaded unless you
          set up an outside service yourself.
        </p>
      </div>

      {/* Right: the form */}
      <div className="flex-1 flex items-center justify-center p-8">
        <form onSubmit={submit} className="w-full max-w-[340px]">
          <h1 className="text-[20px] font-semibold tracking-tight">Set up DevWorkstation</h1>
          <p className="text-[12.5px] text-muted mt-1 mb-6">
            This is the first time DevWorkstation has run on this computer.
            Create the account it will use here, and enter the activation key
            you received when you purchased it.
          </p>

          <label className="block mb-3">
            <span className="block text-[12px] text-muted mb-1">Username</span>
            <input
              className="field mono"
              value={username}
              autoFocus
              autoComplete="username"
              onChange={(e) => setUsername(e.target.value)}
            />
          </label>

          <label className="block mb-3">
            <span className="block text-[12px] text-muted mb-1">Password</span>
            <input
              className="field mono"
              type="password"
              value={password}
              autoComplete="new-password"
              onChange={(e) => setPassword(e.target.value)}
            />
            <span className="block text-[11px] text-muted mt-1">At least 10 characters.</span>
          </label>

          <label className="block mb-4">
            <span className="block text-[12px] text-muted mb-1">Activation key</span>
            <input
              className="field mono"
              value={activationKey}
              autoComplete="off"
              onChange={(e) => setActivationKey(e.target.value)}
            />
          </label>

          {error && (
            <p className="text-[12.5px] mb-3" style={{ color: "var(--err)" }}>
              {error.message}
              {error.recovery && <span className="block text-muted mt-1">{error.recovery}</span>}
              {redirecting && (
                <span className="block text-muted mt-1">
                  If a browser tab didn't open,{" "}
                  <button
                    type="button"
                    className="underline"
                    onClick={() => openUrl(ACTIVATION_PURCHASE_URL).catch(() => undefined)}
                  >
                    open the activation page
                  </button>
                  .
                </span>
              )}
            </p>
          )}

          <button
            className="btn btn-primary w-full"
            disabled={busy || !username || !password || !activationKey}
          >
            {busy ? "Verifying…" : "Create account"}
          </button>

          <p className="text-[11.5px] text-muted mt-5 leading-relaxed">
            This step needs an internet connection so the activation key can
            be verified once. After that, DevWorkstation works offline.
          </p>
        </form>
      </div>
    </div>
  );
}
