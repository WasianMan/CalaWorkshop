import { axiosInstance } from '@/api/axios.ts';

export type RetryFailedResult = {
  retried: number;
  stillFailed: number;
};

/**
 * Re-dispatch every failed download job for the server. `account` applies to all
 * retried items (the original per-item account isn't persisted on the job row).
 */
export default async (serverUuid: string, account?: string | null): Promise<RetryFailedResult> => {
  const { data } = await axiosInstance.post(
    `/api/client/servers/${serverUuid}/calaworkshop/downloads/retry`,
    { account: account ?? null },
  );
  return { retried: data.retried ?? 0, stillFailed: data.still_failed ?? 0 };
};
