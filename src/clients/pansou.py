import requests


class PanSouClient:
    def __init__(self, base_url: str):
        self.base_url = base_url.rstrip('/')

    def search(self, keyword: str, cloud_types: list[str] | None = None, limit: int = 10):
        cloud_types = cloud_types or ["quark"]
        payload = {
            "kw": keyword,
            "res": "merge",
            "cloud_types": cloud_types,
            "src": "all",
        }
        resp = requests.post(f"{self.base_url}/api/search", json=payload, timeout=35)
        resp.raise_for_status()
        data = resp.json()
        merged = data.get("data", {}).get("merged_by_type", {})
        buckets = []
        for cloud_type in cloud_types:
            bucket = []
            for item in merged.get(cloud_type, []) or []:
                item = dict(item)
                item.setdefault("cloud_type", cloud_type)
                bucket.append(item)
            buckets.append(bucket)

        # Round-robin merge so one abundant cloud type (usually Quark) does not
        # consume the whole limit when users select multiple cloud disks.
        items = []
        offset = 0
        while len(items) < limit:
            added = False
            for bucket in buckets:
                if offset < len(bucket):
                    items.append(bucket[offset])
                    added = True
                    if len(items) >= limit:
                        break
            if not added:
                break
            offset += 1
        return items

    def search_quark(self, keyword: str, limit: int = 10):
        return self.search(keyword, ["quark"], limit)
