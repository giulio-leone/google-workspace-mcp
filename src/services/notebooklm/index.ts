import { NotebookLMClient } from './client.js';
import { login } from './auth.js';
import type { HandlerResponse } from '../../server/formatting/markdown.js';

export async function handleNotebookLM(params: Record<string, unknown>): Promise<HandlerResponse> {
  const operation = params.operation as string;
  
  if (operation === 'authenticate') {
    try {
      await login();
      return {
        text: "Successfully authenticated with NotebookLM.",
        refs: { authenticated: true }
      };
    } catch (e: any) {
      return {
        text: `Authentication failed: ${e.message}`,
        refs: { error: true }
      };
    }
  }

  const client = new NotebookLMClient();

  try {
    switch (operation) {
      case 'list': {
        const notebooks = await client.listNotebooks();
        const text = notebooks.map(nb => `- **${nb.title}** (ID: ${nb.id})`).join('\n') || 'No notebooks found.';
        return {
          text: `### Notebooks\n\n${text}`,
          refs: { notebooks }
        };
      }
      
      case 'create': {
        const title = params.title as string;
        if (!title) throw new Error("Title is required for create operation");
        const nb = await client.createNotebook(title);
        return {
          text: `Created notebook: **${nb.title}** (ID: ${nb.id})`,
          refs: { notebook: nb }
        };
      }
      
      case 'get_summary': {
        const notebookId = params.notebookId as string;
        if (!notebookId) throw new Error("notebookId is required");
        const summary = await client.getSummary(notebookId);
        return {
          text: `### Summary for ${notebookId}\n\n${summary}`,
          refs: { summary }
        };
      }

      case 'add_source_url': {
        const notebookId = params.notebookId as string;
        const url = params.url as string;
        if (!notebookId || !url) throw new Error("notebookId and url are required");
        const result = await client.addSourceUrl(notebookId, url);
        return {
          text: `Added source URL ${url} to notebook ${notebookId}.`,
          refs: { result }
        };
      }

      case 'chat': {
        const notebookId = params.notebookId as string;
        const question = params.question as string;
        if (!notebookId || !question) throw new Error("notebookId and question are required");
        const answer = await client.chat(notebookId, question);
        return {
          text: `**Q:** ${question}\n\n**A:** ${answer}`,
          refs: { answer }
        };
      }

      default:
        throw new Error(`Unknown NotebookLM operation: ${operation}`);
    }
  } catch (error: any) {
    return {
      text: `NotebookLM Error: ${error.message}\n(Run 'authenticate' operation if you haven't logged in)`,
      refs: { error: true }
    };
  }
}
