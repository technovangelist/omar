write an application in rust that can compile to a self standing executable that runs on mac, windows, and linux. The app is going to parse log files to look for when different ollama models were last run and how many times they have been run. The log files can be found in a different location on each platform. on mac, they are at ~/.ollama/logs/server*.log. on windows they are at %LOCALAPPDATA%\Ollama. On linux you can get them with the command 'journalctl -e -u ollama'. 

The base directory for models on mac is ~/.ollama/models. on Windows this is %HOMEPATH%\.ollama. On linux this is /usr/share/ollama. That path can be changed with the OLLAMA_MODELS environment variable.

In the logs you will find lines that look something like this: 
llama_model_loader: loaded meta data with 35 key-value pairs and 362 tensors from /Users/matt/.ollama/models/blobs/sha256-1a9a388336073f25f143cdd39abe37b306a367d031d6c04a79bbb545232ae113 (version GGUF V3 (latest))

We are looking for lines that start with "llama_model_loader: loaded meta data". Then record the sha256 hash, without the "sha256-" text. so in this case, its just: 1a9a388336073f25f143cdd39abe37b306a367d031d6c04a79bbb545232ae113

Just above that line is a line that starts with time=2024-10-29T07:18:20.601-07:00. This timestamp should be recorded with that SHA256 hash. This tells us when the model was loaded. 

In the models directory is a tree of folders that is .ollama/models/manifests/REGISTRY/USER/MODEL/TAG. REGISTRY can be one of many different ollama registries. USER can be one of many users on that registry. MODEL can be one of many models that user has created on the registry. And TAG can be one of many tags for that model. Tag is a file with an Ollama manifest that is similar to a docker manifest. Here is an example manifest: 

{
  "schemaVersion": 2,
  "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
  "config": {
    "mediaType": "application/vnd.docker.container.image.v1+json",
    "digest": "sha256:017be051822608223a336d8cb9ecd80f9374f2a68b96e285e636d6e78018a48b",
    "size": 411
  },
  "layers": [
    {
      "mediaType": "application/vnd.ollama.image.model",
      "digest": "sha256:43f7a214e5329f672bb05404cfba1913cbb70fdaa1a17497224e1925046b0ed5",
      "size": 4431388192
    },
    {
      "mediaType": "application/vnd.ollama.image.template",
      "digest": "sha256:64631f1262e4e87d47511bb7b405540321afd297f723f88bf72faae19992ddba",
      "size": 181
    },
    {
      "mediaType": "application/vnd.ollama.image.params",
      "digest": "sha256:db8fbfd0cb288a053f83ac9014ca9bac2558b1bbcd80b5c408a548e7acba8a24",
      "size": 18
    }
  ]
}

In the layers array is a layer with a mediaType of application/vnd.ollama.image.model. The digest of this layer is "sha256:" followed by the sha256 value. that will correspond to the value found in the log file. 

Each model layer digest could potentially be found in many Ollama models. the Name that should be recorded for this model is the parent directories parent / parent directory name : the file name of the manifest. This manifest was found at ~/.ollama/models/manifests/registry.ollama.ai/m/qwen2/7b.q4_0-max. so the modelname should be m/qwen2:7b.q4_0-max. if the parent directories parent name is library, then you can leave that off. 

If the sha256 value from the log file cannot be found in any of the model manifests, then the model is probably deleted. So show the first few characters of the hash and then -deleted. 