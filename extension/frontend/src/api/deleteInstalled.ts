import { axiosInstance } from '@/api/axios.ts';

export default async (serverUuid: string, path: string, files: string[]): Promise<number> => {
  const { data } = await axiosInstance.delete(
    `/api/client/servers/${serverUuid}/calaworkshop/installed`,
    { data: { path, files } },
  );
  return data.deleted ?? 0;
};
