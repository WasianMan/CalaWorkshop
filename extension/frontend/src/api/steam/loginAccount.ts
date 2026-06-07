import { axiosInstance } from '@/api/axios.ts';

export type LoginInput = {
  label: string;
  username: string;
  password: string;
  guardCode?: string | null;
};

export type LoginResult = {
  // 'ok' on success, 'needs_guard' when a Steam Guard code is required (HTTP 409).
  state: 'ok' | 'needs_guard' | string;
  verified?: boolean;
};

export default async (input: LoginInput): Promise<LoginResult> => {
  const { data } = await axiosInstance.post(`/api/client/calaworkshop/steam/accounts`, {
    label: input.label,
    username: input.username,
    password: input.password,
    guard_code: input.guardCode ?? null,
  });
  return data;
};
