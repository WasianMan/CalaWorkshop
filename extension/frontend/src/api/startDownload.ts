import { axiosInstance } from '@/api/axios.ts';

export type StartDownloadInput = {
  appId: number;
  workshopId: number;
  account?: string | null;
  archive?: boolean;
};

export type StartDownloadResult = {
  jobId: string;
  state: string;
};

export default async (serverUuid: string, input: StartDownloadInput): Promise<StartDownloadResult> => {
  // Keys are sent snake_case to match the Rust handler (request bodies are not auto-transformed).
  const { data } = await axiosInstance.post(`/api/client/servers/${serverUuid}/calaworkshop/downloads`, {
    app_id: input.appId,
    workshop_id: input.workshopId,
    account: input.account ?? null,
    archive: input.archive ?? false,
  });
  return data;
};
