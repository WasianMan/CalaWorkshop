import { axiosInstance } from '@/api/axios.ts';

export type DiagnosticCheck = {
  ok: boolean;
  message: string | null;
  error: string | null;
};

export type Diagnostics = {
  helper: DiagnosticCheck;
  steamcmd: DiagnosticCheck;
};

export default async (): Promise<Diagnostics> => {
  const { data } = await axiosInstance.get(`/api/admin/extensions/dev.wasian.calaworkshop/diagnostics`);
  return data;
};
