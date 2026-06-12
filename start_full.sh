#!/bin/bash
cd ~/my-media-sub
source venv/bin/activate
python -c "import uvicorn; uvicorn.run('src.app:app', host='0.0.0.0', port=50001)"
