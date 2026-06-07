import { axiosInstance } from '@/api/axios.ts';

export type InstallResult = {
  installed: boolean;
  fileName: string;
  files: string[];
};

export default async (
  serverUuid: string,
  jobId: string,
  installPath: string,
  archive = false,
): Promise<InstallResult> => {
  const { data } = await axiosInstance.post(
    `/api/client/servers/${serverUuid}/calaworkshop/downloads/${jobId}/install`,
    { install_path: installPath, archive },
  );
  return data;
};
