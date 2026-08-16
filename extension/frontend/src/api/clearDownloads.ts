import { axiosInstance } from '@/api/axios.ts';

/**
 * Clear the server's download history (terminal jobs only) in one action.
 * Pass a state to clear just 'failed' or just 'installed' rows; omit to clear
 * both. Like the per-row remove this only forgets history — installed files,
 * helper artifacts, and Steam are untouched. Returns how many rows were removed.
 */
export default async (
  serverUuid: string,
  state?: 'installed' | 'failed',
): Promise<number> => {
  const { data } = await axiosInstance.post(
    `/api/client/servers/${serverUuid}/calaworkshop/downloads/clear`,
    { state: state ?? null },
  );
  return data.cleared ?? 0;
};
