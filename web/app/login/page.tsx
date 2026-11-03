"use client";

import { useRouter, useSearchParams } from "next/navigation";
import { Suspense, useEffect, useState, type FormEvent } from "react";
import { Alert } from "gui-mellow/Alert";
import { Button } from "gui-mellow/Button";
import { TextField } from "gui-mellow/Field";
import { Pattern } from "gui-mellow/Pattern";
import { SegmentedControl } from "gui-mellow/SegmentedControl";
import { ThemeToggle } from "gui-mellow/ThemeToggle";
import { api, errorMessage, type AuthConfig } from "@/lib/api";
import { Logo } from "@/components/Logo";
import styles from "./login.module.scss";

function safeNext(next: string | null) {
  return next && next.startsWith("/") && !next.startsWith("//") && !next.startsWith("/login") ? next : "/";
}

function LoginForm() {
  const router = useRouter();
  const params = useSearchParams();
  const next = safeNext(params.get("next"));
  const [config, setConfig] = useState<AuthConfig | null>(null);
  const [mode, setMode] = useState<"login" | "register">("login");
  const [error, setError] = useState<string | null>(params.get("error"));
  const [busy, setBusy] = useState(false);
  const [form, setForm] = useState({ email: "", password: "", first_name: "", last_name: "" });

  useEffect(() => {
    api.get<AuthConfig>("/auth/config").then(setConfig, (e) => setError(errorMessage(e)));
    // Already signed in? Skip the form.
    api.get("/me").then(() => router.replace(next), () => {});
  }, [router, next]);

  const set = (key: keyof typeof form) => (e: { target: { value: string } }) =>
    setForm((f) => ({ ...f, [key]: e.target.value }));

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      if (mode === "login") {
        await api.post("/auth/login", { email: form.email, password: form.password });
      } else {
        await api.post("/auth/register", form);
      }
      router.replace(next);
    } catch (err) {
      setError(errorMessage(err));
      setBusy(false);
    }
  }

  return (
    <div className={styles.card}>
      <div className={styles.brand}>
        <Logo size={36} />
        <div>
          <h1 className={styles.title}>Methods</h1>
          <p className={styles.tagline}>Project management for people and their bots.</p>
        </div>
      </div>

      {error && <Alert tone="danger">{error}</Alert>}

      {config?.oidc && (
        <Button
          href={`/api/auth/oidc/start?return_to=${encodeURIComponent(next)}`}
          variant="primary"
          size="lg"
          className={styles.wide}
          prefetch={false}
        >
          Continue with {config.oidc.name}
        </Button>
      )}

      {config?.oidc && config.password_login && <p className={styles.divider}>or use email</p>}

      {config?.password_login && (
        <form onSubmit={submit} className={styles.form}>
          {config.allow_registration && (
            <SegmentedControl
              label="Sign in or create an account"
              value={mode}
              onChange={setMode}
              options={[
                { value: "login", label: "Sign in" },
                { value: "register", label: "Create account" },
              ]}
            />
          )}
          {mode === "register" && (
            <div className={styles.row}>
              <TextField label="First name" value={form.first_name} onChange={set("first_name")} autoComplete="given-name" />
              <TextField label="Last name" value={form.last_name} onChange={set("last_name")} autoComplete="family-name" />
            </div>
          )}
          <TextField
            label="Email"
            type="email"
            required
            value={form.email}
            onChange={set("email")}
            autoComplete="email"
          />
          <TextField
            label="Password"
            type="password"
            required
            minLength={mode === "register" ? 8 : undefined}
            hint={mode === "register" ? "At least 8 characters" : undefined}
            value={form.password}
            onChange={set("password")}
            autoComplete={mode === "login" ? "current-password" : "new-password"}
          />
          <Button type="submit" size="lg" loading={busy} className={styles.wide}>
            {mode === "login" ? "Sign in" : "Create account"}
          </Button>
        </form>
      )}
    </div>
  );
}

export default function LoginPage() {
  return (
    <main className={styles.page}>
      <Pattern pattern="grid" />
      <div className={styles.theme}>
        <ThemeToggle />
      </div>
      <Suspense fallback={<div className="methods-loading">Loading…</div>}>
        <LoginForm />
      </Suspense>
    </main>
  );
}
