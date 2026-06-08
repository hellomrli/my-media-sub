from src.clients.pansou import PanSouClient

client = PanSouClient()
for i, item in enumerate(client.search_quark("盗梦空间"), 1):
    print(i, item.get("note"), item.get("url"), item.get("source"))
