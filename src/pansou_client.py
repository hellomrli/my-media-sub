import requests


class PanSouClient:
    def __init__(self, base_url: str):
        self.base_url = base_url.rstrip('/')

    def search_quark(self, keyword: str, limit: int = 10):
        payload = {
            "kw": keyword,
            "res": "merge",
            "cloud_types": ["quark"],
            "src": "all",
        }
        resp = requests.post(f"{self.base_url}/api/search", json=payload, timeout=35)
        resp.raise_for_status()
        data = resp.json()
        items = data.get("data", {}).get("merged_by_type", {}).get("quark", [])
        return items[:limit]
