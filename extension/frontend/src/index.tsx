import { faPuzzlePiece, faKey } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Extension, ExtensionContext } from 'shared';
import ConfigurationPage from './pages/ConfigurationPage.tsx';
import SteamLinkPage from './pages/SteamLinkPage.tsx';
import WorkshopPage from './pages/WorkshopPage.tsx';

class CalaWorkshopExtension extends Extension {
  public cardConfigurationPage: React.FC | null = ConfigurationPage;
  public cardComponent: React.FC | null = null;

  public initialize(ctx: ExtensionContext): void {
    ctx.extensionRegistry.enterRoutes((routes) => {
      // Per-server Workshop tab.
      routes.addServerRoute({
        name: 'Workshop',
        icon: faPuzzlePiece,
        path: '/calaworkshop',
        element: WorkshopPage,
        permission: 'workshop.read',
      });

      // Account-area page for linking Steam accounts used by downloads.
      routes.addAccountRoute({
        name: 'Steam Link',
        icon: faKey,
        path: '/calaworkshop/steam',
        element: SteamLinkPage,
      });
    });

    ctx.extensionRegistry.permissionIcons.addServerPermissionIcon(
      'workshop',
      <FontAwesomeIcon icon={faPuzzlePiece} />,
    );
  }
}

export default new CalaWorkshopExtension();
