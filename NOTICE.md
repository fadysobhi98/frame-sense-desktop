Frame Sense - third-party components
====================================

Face landmark model (models/face_mesh.onnx)
- Google MediaPipe Face Mesh model, Apache License 2.0
- ONNX conversion: https://github.com/yakhyo/mediapipe-face-mesh-onnx (Apache License 2.0)
- Runs fully offline on the user's machine. Used only to locate facial
  features for the optional face retouch; teeth whitening and skin
  sharpening themselves are deterministic auto-correction, not generative AI.

Eye state model (models/open_closed_eye.onnx)
- Intel OpenVINO open-closed-eye-0001 (Open Model Zoo / openvino_training_extensions),
  Apache License 2.0, trained on the MRL Eye Dataset. Runs fully offline. Used only
  to flag photos where the main faces have closed or lowered eyes so they go to the
  review-only Rejected folder. A small classifier with moderate accuracy, not exact.

Face detection model (models/seeta_fd_frontal_v1.0.bin)
- SeetaFace Detection (rustface / SeetaFace Engine), BSD-style license
  (https://github.com/seetafaceengine/SeetaFace2 - Apache License 2.0)

ONNX Runtime (onnxruntime.dll, onnxruntime_providers_shared.dll)
- Microsoft ONNX Runtime, MIT License
  (https://github.com/microsoft/onnxruntime)

RAW decoding (framesense_raw.dll)
- rawler, LGPL-2.1-only (https://github.com/dnglab/dnglab). Shipped as a
  separate replaceable library; source for our engine is in this
  repository under desktop/src-tauri/raw-engine.

## Auto AI look (3D LUT)
Code: Image-Adaptive-3DLUT (Apache-2.0), https://github.com/HuiZeng/Image-Adaptive-3DLUT.
Weights: trained on MIT-Adobe FiveK (research use only). Included for the owner's personal use only. Do not redistribute this app or sell its output as a product.
