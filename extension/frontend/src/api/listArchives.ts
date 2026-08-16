import { axiosInstance } from '@/api/axios.ts';

export type WorkshopArchive = {
  file: string;
  itemCount: number;
  createdUnix: number;
};

/** List the Workshop archives this server has created (newest first). */
export default async (serverUuid: string): Promise<WorkshopArchive[]> => {
  const { data } = await axiosInstance.get(
    `/api/client/servers/${serverUuid}/calaworkshop/installed/archives`,
  );
  return (data.archives ?? []).map((a: any) => ({
    file: a.file,
    itemCount: a.item_count ?? 0,
    createdUnix: a.created_unix ?? 0,
  }));
};
