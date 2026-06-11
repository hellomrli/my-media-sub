"""推送历史记录服务"""
from __future__ import annotations

import json
from datetime import datetime
from pathlib import Path
from typing import Any


class PushHistoryService:
    """推送历史记录管理"""
    
    def __init__(self, history_file: str = "data/push_history.json"):
        self.history_file = Path(history_file)
        self.history_file.parent.mkdir(parents=True, exist_ok=True)
        self.max_records = 100  # 最多保留100条记录
    
    def add_record(
        self,
        title: str,
        message: str,
        channels: list[str],
        results: dict[str, bool],
        scenario: str = "manual",
    ) -> None:
        """添加推送记录"""
        records = self._load_records()
        
        record = {
            "timestamp": datetime.now().isoformat(),
            "title": title,
            "message": message[:100],  # 仅保存前100字符
            "scenario": scenario,
            "channels": channels,
            "results": results,
            "success_count": sum(1 for v in results.values() if v),
            "total_count": len(results),
        }
        
        records.insert(0, record)  # 新记录在前
        records = records[:self.max_records]  # 保留最新100条
        
        self._save_records(records)
    
    def get_recent(self, limit: int = 50) -> list[dict[str, Any]]:
        """获取最近的推送记录"""
        records = self._load_records()
        return records[:limit]
    
    def get_stats(self) -> dict[str, Any]:
        """获取推送统计"""
        records = self._load_records()
        
        if not records:
            return {
                "total": 0,
                "success": 0,
                "failed": 0,
                "success_rate": 0.0,
                "by_scenario": {},
                "by_channel": {},
            }
        
        total = len(records)
        success = sum(1 for r in records if r.get("success_count", 0) > 0)
        
        # 按场景统计
        by_scenario = {}
        for r in records:
            scenario = r.get("scenario", "unknown")
            by_scenario[scenario] = by_scenario.get(scenario, 0) + 1
        
        # 按渠道统计
        by_channel = {}
        for r in records:
            for channel, ok in r.get("results", {}).items():
                if channel not in by_channel:
                    by_channel[channel] = {"success": 0, "failed": 0}
                if ok:
                    by_channel[channel]["success"] += 1
                else:
                    by_channel[channel]["failed"] += 1
        
        return {
            "total": total,
            "success": success,
            "failed": total - success,
            "success_rate": round(success / total * 100, 1) if total > 0 else 0.0,
            "by_scenario": by_scenario,
            "by_channel": by_channel,
        }
    
    def clear_old_records(self, days: int = 30) -> int:
        """清理N天前的记录"""
        from datetime import timedelta
        
        records = self._load_records()
        cutoff = datetime.now() - timedelta(days=days)
        
        new_records = [
            r for r in records
            if datetime.fromisoformat(r["timestamp"]) > cutoff
        ]
        
        removed = len(records) - len(new_records)
        self._save_records(new_records)
        return removed
    
    def _load_records(self) -> list[dict[str, Any]]:
        """加载历史记录"""
        if not self.history_file.exists():
            return []
        try:
            return json.loads(self.history_file.read_text(encoding="utf-8"))
        except Exception:
            return []
    
    def _save_records(self, records: list[dict[str, Any]]) -> None:
        """保存历史记录"""
        self.history_file.write_text(
            json.dumps(records, ensure_ascii=False, indent=2),
            encoding="utf-8"
        )


# 全局实例
push_history = PushHistoryService()
