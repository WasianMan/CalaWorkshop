import { axiosInstance } from '@/api/axios.ts';

/**
 * Delete every tracked installed Workshop item from the server volume and clear
 * the registry. Returns how many items were removed.
 */
export default async (serverUuid: string): Promise<number> => {
  const { data } = await axiosInstance.post(
    `/api/client/servers/${serverUuid}/calaworkshop/installed/remove-all`,
    {},
  );
  return data.removed ?? 0;
};
