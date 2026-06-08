import requests


class OpenListClient:
    def __init__(self, base_url: str, token: str | None = None):
        self.base_url = base_url.rstrip('/')
        self.token = token

    def headers(self):
        headers = {"Content-Type": "application/json"}
        if self.token:
            headers["Authorization"] = self.token
        return headers

    def login(self, username: str, password: str):
        resp = requests.post(
            f"{self.base_url}/api/auth/login",
            json={"username": username, "password": password},
            timeout=20,
        )
        resp.raise_for_status()
        data = resp.json()
        token = data.get("data", {}).get("token") or data.get("token")
        if not token:
            raise RuntimeError(f"OpenList login did not return token: {data}")
        self.token = token
        return token

    def fs_list(self, path: str):
        payload = {"path": path, "password": "", "page": 1, "per_page": 0, "refresh": False}
        resp = requests.post(f"{self.base_url}/api/fs/list", json=payload, headers=self.headers(), timeout=20)
        resp.raise_for_status()
        return resp.json()

    def fs_get(self, path: str):
        payload = {"path": path, "password": ""}
        resp = requests.post(f"{self.base_url}/api/fs/get", json=payload, headers=self.headers(), timeout=20)
        resp.raise_for_status()
        return resp.json()

    def fs_copy(self, src_dir: str, dst_dir: str, names: list[str], overwrite=False, skip_existing=True, merge=True):
        payload = {
            "src_dir": src_dir,
            "dst_dir": dst_dir,
            "names": names,
            "overwrite": overwrite,
            "skip_existing": skip_existing,
            "merge": merge,
        }
        resp = requests.post(f"{self.base_url}/api/fs/copy", json=payload, headers=self.headers(), timeout=30)
        resp.raise_for_status()
        return resp.json()
