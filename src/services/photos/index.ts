import { runPhotosCli } from './cli.js';
import { getAccessToken } from '../../accounts/token-service.js';
import type { HandlerResponse } from '../../server/formatting/markdown.js';

export async function handlePhotos(params: Record<string, unknown>): Promise<HandlerResponse> {
  const operation = params.operation as string;
  const userId = (params.userId as string) || 'me';

  // Extract real email from userId 'me' using our token service logic or assume standard email input
  // Normally the MCP user provides an email, or we assume they want to use 'me' if they have a default.
  // In `manage_accounts`, it registers emails. For this standalone call, we need a valid email.
  // We'll require the user to pass `email` instead of `userId` since token-service requires an email.
  const email = params.email as string;
  if (!email) {
    throw new Error("The 'email' parameter is required for manage_photos to fetch the correct OAuth token. Use manage_accounts to list available emails.");
  }

  const token = await getAccessToken(email);

  try {
    switch (operation) {
      case 'list_albums': {
        const pageSize = params.pageSize as number || 50;
        const stdout = await runPhotosCli(['list-albums', '--page-size', pageSize.toString()], token);
        const data = JSON.parse(stdout);
        
        return {
          text: `### Google Photos Albums\n\n\`\`\`json\n${JSON.stringify(data, null, 2)}\n\`\`\``,
          refs: { data }
        };
      }
      
      case 'list_media': {
        const albumId = params.albumId as string | undefined;
        const pageSize = params.pageSize as number || 50;
        
        const args = ['list-media', '--page-size', pageSize.toString()];
        if (albumId) {
          args.push('--album-id', albumId);
        }
        
        const stdout = await runPhotosCli(args, token);
        const data = JSON.parse(stdout);
        
        return {
          text: `### Google Photos Media Items\n\n\`\`\`json\n${JSON.stringify(data, null, 2)}\n\`\`\``,
          refs: { data }
        };
      }
      
      case 'get_media': {
        const mediaItemId = params.mediaItemId as string;
        if (!mediaItemId) throw new Error("mediaItemId is required for get_media");
        
        const stdout = await runPhotosCli(['get-media', '--media-item-id', mediaItemId], token);
        const data = JSON.parse(stdout);
        
        return {
          text: `### Media Item Details\n\n\`\`\`json\n${JSON.stringify(data, null, 2)}\n\`\`\``,
          refs: { data }
        };
      }

      default:
        throw new Error(`Unknown manage_photos operation: ${operation}`);
    }
  } catch (error: any) {
    return {
      text: `Google Photos Error: ${error.message}`,
      refs: { error: true }
    };
  }
}
