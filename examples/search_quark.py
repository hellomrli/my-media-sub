from src.pansou_client import PanSouClient

client = PanSouClient("https://pansou.lxf87.com.cn")
for i, item in enumerate(client.search_quark("盗梦空间"), 1):
    print(i, item.get("note"), item.get("url"), item.get("source"))
