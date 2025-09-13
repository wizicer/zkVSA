// Type definitions
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

// Sample data for the samples table
export const samples: Sample[] = [
  {
    transcript: "Hello, this is a sample transcript for demonstration purposes.",
    originalSpeech: "sample1_original.wav",
    anonymizedSpeech: {
      "3":{
        f32: "sample1_f32.wav",
        m32: "sample1_m32.wav", 
        m377: "sample1_m377.wav"
      },
      "2":{
        f32: "sample1_f32.wav",
        m32: "sample1_m32.wav", 
        m377: "sample1_m377.wav"
      },
      "1":{
        f32: "sample1_f32.wav",
        m32: "sample1_m32.wav", 
        m377: "sample1_m377.wav"
      },
      "-1":{
        f32: "sample1_f32.wav",
        m32: "sample1_m32.wav", 
        m377: "sample1_m377.wav"
      },
      "-2":{
        f32: "sample1_f32.wav",
        m32: "sample1_m32.wav", 
        m377: "sample1_m377.wav"
      },
      "-3":{
        f32: "sample1_f32.wav",
        m32: "sample1_m32.wav", 
        m377: "sample1_m377.wav"
      }
    },
    proof: {
      "3": { size: "2.5 KB" },
      "2": { size: "2.5 KB" },
      "1": { size: "2.5 KB" },
      "-1": { size: "2.5 KB" },
      "-2": { size: "2.5 KB" },
      "-3": { size: "2.5 KB" }
    }
  },
  {
    transcript: "Another example transcript to show the anonymization capabilities.",
    originalSpeech: "sample2_original.wav",
    anonymizedSpeech: {
      "3":{
        f32: "sample2_f32.wav",
        m32: "sample2_m32.wav", 
        m377: "sample2_m377.wav"
      },
      "2":{
        f32: "sample2_f32.wav",
        m32: "sample2_m32.wav", 
        m377: "sample2_m377.wav"
      },
      "1":{
        f32: "sample2_f32.wav",
        m32: "sample2_m32.wav", 
        m377: "sample2_m377.wav"
      },
      "-1":{
        f32: "sample2_f32.wav",
        m32: "sample2_m32.wav", 
        m377: "sample2_m377.wav"
      },
      "-2":{
        f32: "sample2_f32.wav",
        m32: "sample2_m32.wav", 
        m377: "sample2_m377.wav"
      },
      "-3":{
        f32: "sample2_f32.wav",
        m32: "sample2_m32.wav", 
        m377: "sample2_m377.wav"
      }
    },
    proof: {
      "3": { size: "2.6 KB" },
      "2": { size: "2.6 KB" },
      "1": { size: "2.6 KB" },
      "-1": { size: "2.6 KB" },
      "-2": { size: "2.6 KB" },
      "-3": { size: "2.6 KB" }
    }
  }
];
