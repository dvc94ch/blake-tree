#!/usr/bin/env python
import pdfplumber
import pytesseract
from PIL import Image
import os
import sys
import onnx
import torch
import onnxruntime
from omegaconf import OmegaConf

input = sys.argv[1]

name, ext = os.path.splitext(input)

if ext == "pdf":
    with pdfplumber.open(input) as pdf:
        with open(output) as f:
            for page in pdf.pages:
                text = page.extract_text_simple(x_tolerance=3, y_tolerance=3)
                f.write(text)
elif ext == "png" or ext == "jpg":
    image = Image.open(input)
    text = pytesseract.image_to_string(image)
    with open(output) as f:
        f.write(text)
elif ext == "webm" or ext == "weba":
    os.exec("ffmpeg -i %s -vn -acodec copy %s.ogg" % (input, name))
    language = 'en' # also available 'de', 'es'

    # load provided utils
    _, decoder, utils = torch.hub.load(repo_or_dir='snakers4/silero-models', model='silero_stt', language=language)
    (read_batch, split_into_batches,
         read_audio, prepare_model_input) = utils

    # see available models
    torch.hub.download_url_to_file('https://raw.githubusercontent.com/snakers4/silero-models/master/models.yml', 'models.yml')
    models = OmegaConf.load('models.yml')
    available_languages = list(models.stt_models.keys())
    assert language in available_languages

    # load the actual ONNX model
    torch.hub.download_url_to_file(models.stt_models.en.latest.onnx, 'model.onnx', progress=True)
    onnx_model = onnx.load('model.onnx')
    onnx.checker.check_model(onnx_model)
    ort_session = onnxruntime.InferenceSession('model.onnx')

    # download a single file in any format compatible with TorchAudio
    test_files = [input]
    batches = split_into_batches(test_files, batch_size=10)
    input = prepare_model_input(read_batch(batches[0]))

    # actual ONNX inference and decoding
    onnx_input = input.detach().cpu().numpy()
    ort_inputs = {'input': onnx_input}
    ort_outs = ort_session.run(None, ort_inputs)
    decoded = decoder(torch.Tensor(ort_outs[0])[0])

    with open(output) as f:
        f.write(decoded)