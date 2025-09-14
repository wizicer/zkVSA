// Type declarations for WASM integration
declare global {
  interface Window {
    verifyProof: (proofData: string) => { success: boolean; error?: string; message?: string };
    Go: any;
  }
}

export {};
