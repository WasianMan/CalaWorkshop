import { axiosInstance } from '@/api/axios.ts';

export default async (serverUuid: string, jobId: string): Promise<void> => {
  await axiosInstance.delete(
    `/api/client/servers/${serverUuid}/calaworkshop/downloads/${jobId}`,
  );
};
