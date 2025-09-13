// Type definitions for ZK-VSA samples
export interface AnonymizedSpeech {
  f32: string;
  m32: string;
  m377: string;
}

export interface Proof {
  size: string;
}

export interface Sample {
  transcript: string;
  originalSpeech: string;
  anonymizedSpeech: {
    "3": AnonymizedSpeech;
    "2": AnonymizedSpeech;
    "1": AnonymizedSpeech;
    "-1": AnonymizedSpeech;
    "-2": AnonymizedSpeech;
    "-3": AnonymizedSpeech;
  };
  proof: {
    "3": Proof;
    "2": Proof;
    "1": Proof;
    "-1": Proof;
    "-2": Proof;
    "-3": Proof;
  };
}
