#!/usr/bin/env python3
"""
Lokal LLM client adapter.
Stödjer: Ollama, LM Studio, llama.cpp
"""

import json
import sys
import os
import requests
import time
from typing import List, Dict, Any, Optional
from enum import Enum

class LLMProvider(Enum):
    OLLAMA = "ollama"
    LMSTUDIO = "lmstudio"
    LLAMACPP = "llamacpp"

class LocalLLMClient:
    def __init__(self):
        self.provider = os.getenv("LOCAL_LLM_PROVIDER", "ollama").lower()
        self.base_url = os.getenv("LOCAL_LLM_BASE_URL", "")
        self.model = os.getenv("LOCAL_LLM_MODEL", "")
        self.temperature = float(os.getenv("LOCAL_LLM_TEMPERATURE", "0"))
        self.max_tokens = int(os.getenv("LOCAL_LLM_MAX_TOKENS", "2048"))
        self.timeout = int(os.getenv("LOCAL_LLM_TIMEOUT", "120"))
        
        # Set defaults per provider
        if not self.base_url:
            if self.provider == "ollama":
                self.base_url = "http://localhost:11434"
            elif self.provider == "lmstudio":
                self.base_url = "http://localhost:1234"
            elif self.provider == "llamacpp":
                self.base_url = "http://localhost:8080"
            else:
                raise ValueError(f"Unknown provider: {self.provider}")
        
        if not self.model:
            raise ValueError("LOCAL_LLM_MODEL must be set")
    
    def _check_server(self) -> bool:
        """Kontrollera om LLM-server är tillgänglig"""
        try:
            if self.provider == "ollama":
                response = requests.get(f"{self.base_url}/api/tags", timeout=5)
            elif self.provider == "lmstudio":
                response = requests.get(f"{self.base_url}/v1/models", timeout=5)
            elif self.provider == "llamacpp":
                response = requests.get(f"{self.base_url}/health", timeout=5)
            else:
                return False
            return response.status_code == 200
        except Exception:
            return False

    def _ollama_model_error(self, response: requests.Response) -> Optional[str]:
        """Returnera felmeddelande om Ollama svarar med model-fel."""
        try:
            payload = response.json()
        except ValueError:
            return None
        error_msg = str(payload.get("error", "")).lower()
        if "model" in error_msg and ("not found" in error_msg or "required" in error_msg):
            return error_msg
        return None

    def _ollama_available_models(self) -> List[str]:
        """Hämta tillgängliga Ollama-modeller."""
        try:
            response = requests.get(f"{self.base_url}/api/tags", timeout=5)
            response.raise_for_status()
            data = response.json()
            return [entry.get("name", "") for entry in data.get("models", []) if entry.get("name")]
        except Exception:
            return []
    
    def chat(self, messages: List[Dict[str, str]], retries: int = 3) -> str:
        """
        Skicka chat request till lokal LLM.
        
        Args:
            messages: Lista med {"role": "user|assistant|system", "content": "..."}
            retries: Antal retries vid timeout
        
        Returns:
            Response text från LLM
        """
        # Kontrollera server
        if not self._check_server():
            error_msg = f"LLM server not available at {self.base_url}\n"
            if self.provider == "ollama":
                error_msg += "Start with: ollama serve"
            elif self.provider == "lmstudio":
                error_msg += "Start LM Studio server on port 1234"
            elif self.provider == "llamacpp":
                error_msg += "Start llama.cpp server on port 8080"
            raise ConnectionError(error_msg)
        
        for attempt in range(retries):
            try:
                if self.provider == "ollama":
                    return self._chat_ollama(messages)
                elif self.provider == "lmstudio":
                    return self._chat_lmstudio(messages)
                elif self.provider == "llamacpp":
                    return self._chat_llamacpp(messages)
                else:
                    raise ValueError(f"Unknown provider: {self.provider}")
            except requests.exceptions.Timeout:
                if attempt < retries - 1:
                    wait_time = 2 ** attempt
                    print(f"  ⏳ Timeout, retry {attempt + 1}/{retries} efter {wait_time}s...", file=sys.stderr)
                    time.sleep(wait_time)
                else:
                    raise
            except Exception as e:
                if attempt < retries - 1:
                    wait_time = 2 ** attempt
                    print(f"  ⏳ Error: {e}, retry {attempt + 1}/{retries} efter {wait_time}s...", file=sys.stderr)
                    time.sleep(wait_time)
                else:
                    raise
    
    def _chat_ollama(self, messages: List[Dict[str, str]]) -> str:
        """Chat via Ollama API"""
        url = f"{self.base_url}/api/chat"
        
        payload = {
            "model": self.model,
            "messages": messages,
            "temperature": self.temperature,
            "stream": False
        }
        
        response = requests.post(url, json=payload, timeout=self.timeout)
        model_error = self._ollama_model_error(response)
        if model_error:
            available = self._ollama_available_models()
            available_str = ", ".join(available) if available else "No models found (run `ollama list`)"
            raise ValueError(
                f"Ollama model error: {model_error}. "
                f"Set LOCAL_LLM_MODEL to one of: {available_str}"
            )
        if response.status_code == 404:
            return self._generate_ollama(messages)
        try:
            response.raise_for_status()
        except requests.exceptions.HTTPError as exc:
            status_code = exc.response.status_code if exc.response else None
            if status_code == 404:
                return self._generate_ollama(messages)
            raise

        data = response.json()
        return data.get("message", {}).get("content", "")

    def _generate_ollama(self, messages: List[Dict[str, str]]) -> str:
        """Fallback för äldre Ollama: använd /api/generate."""
        url = f"{self.base_url}/api/generate"

        prompt = ""
        for msg in messages:
            role = msg.get("role", "")
            content = msg.get("content", "")
            if role == "system":
                prompt += f"System: {content}\n\n"
            elif role == "user":
                prompt += f"User: {content}\n\n"
            elif role == "assistant":
                prompt += f"Assistant: {content}\n\n"
        prompt += "Assistant:"

        payload = {
            "model": self.model,
            "prompt": prompt,
            "temperature": self.temperature,
            "stream": False
        }

        response = requests.post(url, json=payload, timeout=self.timeout)
        model_error = self._ollama_model_error(response)
        if model_error:
            available = self._ollama_available_models()
            available_str = ", ".join(available) if available else "No models found (run `ollama list`)"
            raise ValueError(
                f"Ollama model error: {model_error}. "
                f"Set LOCAL_LLM_MODEL to one of: {available_str}"
            )
        if response.status_code == 404:
            raise RuntimeError(
                f"Ollama endpoint not found at {url}. "
                "Your server may not support /api/chat or /api/generate. "
                "Check the base URL and Ollama version."
            )
        response.raise_for_status()

        data = response.json()
        return data.get("response", "")
    
    def _chat_lmstudio(self, messages: List[Dict[str, str]]) -> str:
        """Chat via LM Studio (OpenAI-compatible) API"""
        url = f"{self.base_url}/v1/chat/completions"
        
        payload = {
            "model": self.model,
            "messages": messages,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens
        }
        
        response = requests.post(url, json=payload, timeout=self.timeout)
        response.raise_for_status()
        
        data = response.json()
        return data.get("choices", [{}])[0].get("message", {}).get("content", "")
    
    def _chat_llamacpp(self, messages: List[Dict[str, str]]) -> str:
        """Chat via llama.cpp server API"""
        # llama.cpp använder completion API, inte chat
        # Konvertera messages till prompt
        prompt = ""
        for msg in messages:
            role = msg.get("role", "")
            content = msg.get("content", "")
            if role == "system":
                prompt += f"System: {content}\n\n"
            elif role == "user":
                prompt += f"User: {content}\n\n"
            elif role == "assistant":
                prompt += f"Assistant: {content}\n\n"
        prompt += "Assistant:"
        
        url = f"{self.base_url}/completion"
        
        payload = {
            "prompt": prompt,
            "temperature": self.temperature,
            "n_predict": self.max_tokens
        }
        
        response = requests.post(url, json=payload, timeout=self.timeout)
        response.raise_for_status()
        
        data = response.json()
        return data.get("content", "")

def get_client() -> LocalLLMClient:
    """Hämta konfigurerad LLM client"""
    return LocalLLMClient()
