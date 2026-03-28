import { fetchTokens } from './auth.js';

const BATCHEXECUTE_URL = "https://notebooklm.google.com/_/LabsTailwindUi/data/batchexecute";

export enum RPCMethod {
  LIST_NOTEBOOKS = "wXbhsf",
  CREATE_NOTEBOOK = "CCqFvf",
  GET_NOTEBOOK = "rLM1Ne",
  DELETE_NOTEBOOK = "WWINqb",
  ADD_SOURCE = "izAoDd",
  SUMMARIZE = "VfAZjd"
}

export class NotebookLMClient {
  private csrfToken: string = '';
  private sessionId: string = '';
  private cookieHeader: string = '';
  private initialized: boolean = false;

  async init() {
    if (this.initialized) return;
    const tokens = await fetchTokens();
    this.csrfToken = tokens.csrfToken;
    this.sessionId = tokens.sessionId;
    this.cookieHeader = tokens.cookieHeader;
    this.initialized = true;
  }

  private encodeRpcRequest(method: RPCMethod, params: any[]): any[] {
    const paramsJson = JSON.stringify(params);
    const inner = [method, paramsJson, null, "generic"];
    return [[inner]];
  }

  private buildRequestBody(rpcRequest: any[]): string {
    const fReq = JSON.stringify(rpcRequest);
    const bodyParts = [`f.req=${encodeURIComponent(fReq)}`];
    if (this.csrfToken) {
      bodyParts.push(`at=${encodeURIComponent(this.csrfToken)}`);
    }
    return bodyParts.join('&') + '&';
  }

  private parseChunkedResponse(response: string): any[] {
    let text = response;
    // Strip anti-XSSI
    if (text.startsWith(")]}'")) {
      text = text.substring(text.indexOf('\n') + 1);
    }
    
    const chunks: any[] = [];
    const lines = text.trim().split('\n');
    let i = 0;
    while (i < lines.length) {
      const line = lines[i].trim();
      if (!line) {
        i++;
        continue;
      }
      // Check if line is a byte count
      if (!isNaN(Number(line))) {
        i++;
        if (i < lines.length) {
          try {
            chunks.push(JSON.parse(lines[i]));
          } catch (e) {
            // Ignore malformed chunk
          }
        }
        i++;
      } else {
        try {
          chunks.push(JSON.parse(line));
        } catch (e) {
          // Ignore
        }
        i++;
      }
    }
    return chunks;
  }

  private extractRpcResult(chunks: any[], rpcId: string): any {
    for (const chunk of chunks) {
      if (!Array.isArray(chunk)) continue;
      const items = (chunk.length > 0 && Array.isArray(chunk[0])) ? chunk : [chunk];
      
      for (const item of items) {
        if (!Array.isArray(item) || item.length < 3) continue;
        if (item[0] === 'er' && item[1] === rpcId) {
          throw new Error(`RPC Error: ${item[2]}`);
        }
        if (item[0] === 'wrb.fr' && item[1] === rpcId) {
          const resultData = item[2];
          if (typeof resultData === 'string') {
            try {
              return JSON.parse(resultData);
            } catch (e) {
              return resultData;
            }
          }
          return resultData;
        }
      }
    }
    return null;
  }

  async rpcCall(method: RPCMethod, params: any[], sourcePath: string = '/'): Promise<any> {
    await this.init();

    const urlParams = new URLSearchParams({
      rpcids: method,
      'source-path': sourcePath,
      'f.sid': this.sessionId,
      rt: 'c'
    });
    
    const url = `${BATCHEXECUTE_URL}?${urlParams.toString()}`;
    const rpcReq = this.encodeRpcRequest(method, params);
    const body = this.buildRequestBody(rpcReq);

    const res = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded;charset=UTF-8',
        'Cookie': this.cookieHeader,
        'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36'
      },
      body
    });

    if (!res.ok) {
      throw new Error(`NotebookLM RPC Failed: ${res.status} ${res.statusText}`);
    }

    const resText = await res.text();
    const chunks = this.parseChunkedResponse(resText);
    const result = this.extractRpcResult(chunks, method);
    return result;
  }

  async listNotebooks() {
    const result = await this.rpcCall(RPCMethod.LIST_NOTEBOOKS, [null, 1, null, [2]]);
    if (Array.isArray(result) && result.length > 0) {
      const rawNotebooks = Array.isArray(result[0]) ? result[0] : result;
      return rawNotebooks.map((nb: any) => ({
        id: nb[0],
        title: nb[1],
        createdAt: nb[2]
      }));
    }
    return [];
  }

  async createNotebook(title: string) {
    const result = await this.rpcCall(RPCMethod.CREATE_NOTEBOOK, [title, null, null, [2], [1]]);
    return {
      id: result[0],
      title: result[1]
    };
  }

  async getSummary(notebookId: string) {
    const result = await this.rpcCall(RPCMethod.SUMMARIZE, [notebookId, [2]], `/notebook/${notebookId}`);
    try {
      if (Array.isArray(result)) {
        return result[0][0][0];
      }
    } catch (e) {}
    return "No summary available.";
  }

  async addSourceUrl(notebookId: string, url: string) {
    const params = [notebookId, 1, null, [url]];
    const result = await this.rpcCall(RPCMethod.ADD_SOURCE, params, `/notebook/${notebookId}`);
    return result;
  }

  async chat(notebookId: string, question: string) {
    await this.init();
    // Use an empty sources array to query all sources by default
    const params = [
      [],
      question,
      null, // No history yet
      [2, null, [1], [1]],
      null, // No conversation ID
      null,
      null,
      notebookId,
      1
    ];
    
    const paramsJson = JSON.stringify(params);
    const fReq = [null, paramsJson];
    const fReqJson = JSON.stringify(fReq);
    
    const encodedReq = encodeURIComponent(fReqJson);
    const bodyParts = [`f.req=${encodedReq}`];
    if (this.csrfToken) {
      bodyParts.push(`at=${encodeURIComponent(this.csrfToken)}`);
    }
    const body = bodyParts.join('&') + '&';

    const urlParams = new URLSearchParams({
      hl: 'en',
      _reqid: Math.floor(Math.random() * 1000000).toString(),
      rt: 'c',
      'f.sid': this.sessionId
    });

    const url = `https://notebooklm.google.com/_/LabsTailwindUi/data/google.internal.labs.tailwind.orchestration.v1.LabsTailwindOrchestrationService/GenerateFreeFormStreamed?${urlParams.toString()}`;

    const res = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded;charset=UTF-8',
        'Cookie': this.cookieHeader,
        'User-Agent': 'Mozilla/5.0'
      },
      body
    });

    if (!res.ok) {
      throw new Error(`NotebookLM Chat Failed: ${res.status} ${res.statusText}`);
    }

    const resText = await res.text();
    const chunks = this.parseChunkedResponse(resText);
    
    // Find the answer text. Usually it's in the wrb.fr payload with [text, null, [convId, ...], ...]
    let bestAnswer = "";
    for (const chunk of chunks) {
      if (!Array.isArray(chunk)) continue;
      const items = (chunk.length > 0 && Array.isArray(chunk[0])) ? chunk : [chunk];
      
      for (const item of items) {
        if (!Array.isArray(item) || item.length < 3) continue;
        if (item[0] === 'wrb.fr') {
          const innerJson = item[2];
          if (typeof innerJson === 'string') {
            try {
              const innerData = JSON.parse(innerJson);
              if (Array.isArray(innerData) && innerData.length > 0) {
                const first = innerData[0];
                if (Array.isArray(first) && first.length > 0 && typeof first[0] === 'string') {
                   // Keep updating to get the longest/best answer as it streams
                   if (first[0].length > bestAnswer.length) {
                     bestAnswer = first[0];
                   }
                }
              }
            } catch (e) {
              // Ignore
            }
          }
        }
      }
    }
    return bestAnswer || "No answer received.";
  }
}
