import { axiosInstance } from '@/api/axios.ts';

export default async (serverUuid: string, installedId: string): Promise<number> => {
  const { data } = await axiosInstance.delete(
    `/api/client/servers/${serverUuid}/calaworkshop/installed/${installedId}`,
  );
  return data.deleted ?? 0;
};
