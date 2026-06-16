import { axiosInstance } from '@/api/axios.ts';

export type ArchiveResult = {
  archived: number;
  file: string;
};

/**
 * Compress every tracked installed Workshop item into a single archive at the
 * server volume root. An optional `name` is sanitized server-side (`.tar.gz` is
 * appended if missing). Returns how many items were archived and the file name.
 */
export default async (serverUuid: string, name?: string | null): Promise<ArchiveResult> => {
  const { data } = await axiosInstance.post(
    `/api/client/servers/${serverUuid}/calaworkshop/installed/archive`,
    { name: name?.trim() || null },
  );
  return { archived: data.archived ?? 0, file: data.file ?? '' };
};
