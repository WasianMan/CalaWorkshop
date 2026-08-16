import { axiosInstance } from '@/api/axios.ts';

/**
 * Cancel every in-flight (queued/downloading) download job for the server by
 * marking it failed. Database-only — no helper or Steam calls — so it's safe for
 * clearing a stuck backlog. Returns how many jobs were cancelled.
 */
export default async (serverUuid: string): Promise<number> => {
  const { data } = await axiosInstance.post(
    `/api/client/servers/${serverUuid}/calaworkshop/downloads/cancel`,
    {},
  );
  return data.cancelled ?? 0;
};
