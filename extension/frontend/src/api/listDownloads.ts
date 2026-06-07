import { axiosInstance } from '@/api/axios.ts';
import type { WorkshopJob } from './getJob.ts';

export default async (serverUuid: string): Promise<WorkshopJob[]> => {
  const { data } = await axiosInstance.get(`/api/client/servers/${serverUuid}/calaworkshop/downloads`);
  return data.jobs ?? [];
};
