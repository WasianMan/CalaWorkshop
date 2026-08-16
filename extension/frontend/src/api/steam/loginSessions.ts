import { axiosInstance } from '@/api/axios.ts';

// Async login session, as reported by the helper. `state` values are:
// running, awaiting_mobile_confirmation, awaiting_qr, needs_guard, verifying,
// ok, failed. On failure `errorKind` classifies it (invalid_credentials,
// rate_limited, connectivity, timeout, expired, qr_unsupported, superseded,
// internal) and `error` is a human-readable message.
export type LoginSession = {
  id: string;
  method: 'password' | 'qr' | string;
  state:
    | 'running'
    | 'awaiting_mobile_confirmation'
    | 'awaiting_qr'
    | 'needs_guard'
    | 'verifying'
    | 'ok'
    | 'failed'
    | string;
  username?: string | null;
  error?: string | null;
  errorKind?: string | null;
  // For needs_guard: where the code is ('email' | 'device').
  guardHint?: string | null;
  challengeUrl?: string | null;
  // Ready-to-display SVG of the QR code (QR sessions only).
  qrSvg?: string | null;
  verified?: boolean;
};

export type BeginPasswordInput = {
  label: string;
  username: string;
  password: string;
  guardCode?: string | null;
};

/** Map the helper's raw snake_case session view to the camelCase type. */
const sessionFromWire = (s: any): LoginSession => ({
  id: s.id,
  method: s.method,
  state: s.state,
  username: s.username ?? null,
  error: s.error ?? null,
  errorKind: s.error_kind ?? null,
  guardHint: s.guard_hint ?? null,
  challengeUrl: s.challenge_url ?? null,
  qrSvg: s.qr_svg ?? null,
  verified: s.verified ?? false,
});

export async function beginPasswordSession(input: BeginPasswordInput): Promise<LoginSession> {
  const { data } = await axiosInstance.post(`/api/client/calaworkshop/steam/login-sessions`, {
    label: input.label,
    username: input.username,
    password: input.password,
    guard_code: input.guardCode ?? null,
  });
  return sessionFromWire(data);
}

export async function beginQrSession(label: string): Promise<LoginSession> {
  const { data } = await axiosInstance.post(`/api/client/calaworkshop/steam/login-sessions/qr`, {
    label,
  });
  return sessionFromWire(data);
}

export async function getLoginSession(id: string, label: string): Promise<LoginSession> {
  const { data } = await axiosInstance.get(
    `/api/client/calaworkshop/steam/login-sessions/${encodeURIComponent(id)}?label=${encodeURIComponent(label)}`,
  );
  return sessionFromWire(data);
}

export async function cancelLoginSession(id: string, label: string): Promise<void> {
  await axiosInstance.delete(
    `/api/client/calaworkshop/steam/login-sessions/${encodeURIComponent(id)}?label=${encodeURIComponent(label)}`,
  );
}
