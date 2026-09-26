import { useState } from "react";
import { call, describe, AppError, DashboardData, SessionUser } from "../lib/api";
import { Panel, ErrorNotice, Field, Row } from "../components/ui";

export default function Settings({
  user, onUser, summary,
}: { user: SessionUser; onUser: (u: SessionUser) => void; summary: DashboardData | null }) {
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [pwError, setPwError] = useState<AppError | null>(null);
  const [pwDone, setPwDone] = useState("");

  const [accountPassword, setAccountPassword] = useState("");
  const [username, setUsername] = useState(user.username);
  const [displayName, setDisplayName] = useState(user.display_name);
  const [acError, setAcError] = useState<AppError | null>(null);
  const [acDone, setAcDone] = useState("");

  const changePassword = async (e: React.FormEvent) => {
    e.preventDefault();
    setPwError(null); setPwDone("");
    if (newPassword !== confirmPassword) {
      setPwError({
        status: "error", code: "PASSWORDS_DIFFER",
        message: "The two new passwords are not the same.",
        recovery: "Retype them and try again.",
      });
      return;
    }
    try {
      await call("change_password", { currentPassword, newPassword });
      onUser({ ...user, password_changed: true });
      setCurrentPassword(""); setNewPassword(""); setConfirmPassword("");
      setPwDone("Password changed. Use it the next time you sign in.");
    } catch (err) { setPwError(describe(err)); }
  };

  const changeAccount = async (e: React.FormEvent) => {
    e.preventDefault();
    setAcError(null); setAcDone("");
    try {
      const updated = await call<SessionUser>("change_username", {
        password: accountPassword, newUsername: username, displayName,
      });
      onUser(updated);
      setAccountPassword("");
      setAcDone("Account details updated.");
    } catch (err) { setAcError(describe(err)); }
  };

  return (
    <div className="max-w-[820px] space-y-4">
      {!user.password_changed && (
        <div className="panel p-3 text-[12.5px]" style={{ borderColor: "var(--accent)" }}>
          <span style={{ color: "var(--accent)" }}>This account still uses its original password.</span>
          <span className="text-muted"> Change it below so the only person who knows it is you.</span>
        </div>
      )}

      <Panel title="Password">
        <form onSubmit={changePassword} className="space-y-3 max-w-[380px]">
          <Field label="Current password">
            <input className="field mono" type="password" autoComplete="current-password"
              value={currentPassword} onChange={(e) => setCurrentPassword(e.target.value)} />
          </Field>
          <Field label="New password" hint="At least 10 characters. Three or four unrelated words work well.">
            <input className="field mono" type="password" autoComplete="new-password"
              value={newPassword} onChange={(e) => setNewPassword(e.target.value)} />
          </Field>
          <Field label="New password again">
            <input className="field mono" type="password" autoComplete="new-password"
              value={confirmPassword} onChange={(e) => setConfirmPassword(e.target.value)} />
          </Field>
          {pwError && <ErrorNotice error={pwError} />}
          {pwDone && <p className="text-[12.5px]" style={{ color: "var(--ok)" }}>{pwDone}</p>}
          <button className="btn btn-primary" disabled={!currentPassword || !newPassword}>Change password</button>
        </form>
      </Panel>

      <Panel title="Account">
        <form onSubmit={changeAccount} className="space-y-3 max-w-[380px]">
          <Field label="Username">
            <input className="field mono" value={username} onChange={(e) => setUsername(e.target.value)} />
          </Field>
          <Field label="Display name">
            <input className="field" value={displayName} onChange={(e) => setDisplayName(e.target.value)} />
          </Field>
          <Field label="Confirm with your password">
            <input className="field mono" type="password" value={accountPassword}
              onChange={(e) => setAccountPassword(e.target.value)} />
          </Field>
          {acError && <ErrorNotice error={acError} />}
          {acDone && <p className="text-[12.5px]" style={{ color: "var(--ok)" }}>{acDone}</p>}
          <button className="btn" disabled={!accountPassword || !username}>Save account details</button>
        </form>
      </Panel>

      <Panel title="Storage on this computer">
        <Row label="Data folder" value={summary?.data_folder ?? "—"} />
        <Row label="Local database" value="workstation.sqlite3 — projects, servers, CVs, logs, settings" />
        <Row label="Credentials" value="Windows Credential Manager — never in the folder above" />
        <Row label="Password storage" value="Argon2id hash. The password itself is not stored anywhere." />
        <p className="text-[12px] text-muted mt-3 max-w-[76ch]">
          Backing up the data folder copies your projects, CVs and settings but
          not your saved credentials, which stay with the Windows account.
        </p>
      </Panel>
    </div>
  );
}
