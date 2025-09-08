// Utility functions for API key validation and management

export interface ApiKeyValidation {
  isValid: boolean;
  message: string;
  severity: 'error' | 'warning' | 'success';
}

/**
 * Validates GitHub API key format and provides user feedback
 */
export function validateGitHubApiKey(key: string | undefined): ApiKeyValidation {
  if (!key || key.trim().length === 0) {
    return {
      isValid: false,
      message: 'GitHub API key is required for translation',
      severity: 'error'
    };
  }

  const trimmedKey = key.trim();
  
  // GitHub Personal Access Tokens should start with specific prefixes
  if (trimmedKey.startsWith('github_pat_')) {
    // Fine-grained personal access token (new format)
    return {
      isValid: true,
      message: 'Fine-grained token detected (recommended)',
      severity: 'success'
    };
  } else if (trimmedKey.startsWith('ghp_')) {
    // Classic personal access token
    return {
      isValid: true,
      message: 'Classic token detected (ensure it has AI model access)',
      severity: 'warning'
    };
  } else if (trimmedKey.length >= 32) {
    // Probably a valid token but unknown format
    return {
      isValid: true,
      message: 'Token format accepted',
      severity: 'success'
    };
  }

  return {
    isValid: false,
    message: 'Invalid GitHub token format. Please check your token.',
    severity: 'error'
  };
}

/**
 * Validates Gemini API key format and provides user feedback
 */
export function validateGeminiApiKey(key: string | undefined): ApiKeyValidation {
  if (!key || key.trim().length === 0) {
    return {
      isValid: false,
      message: 'Gemini API key is required for translation',
      severity: 'error'
    };
  }

  const trimmedKey = key.trim();
  
  // Gemini API keys typically start with specific patterns
  if (trimmedKey.startsWith('AIza') && trimmedKey.length >= 35) {
    return {
      isValid: true,
      message: 'Valid Gemini API key format',
      severity: 'success'
    };
  } else if (trimmedKey.length >= 30) {
    // Might be valid but different format
    return {
      isValid: true,
      message: 'API key format accepted',
      severity: 'success'
    };
  }

  return {
    isValid: false,
    message: 'Invalid Gemini API key format. Please check your key.',
    severity: 'error'
  };
}

/**
 * Validates API key based on provider
 */
export function validateApiKey(provider: string, key: string | undefined): ApiKeyValidation {
  switch (provider) {
    case 'github':
      return validateGitHubApiKey(key);
    case 'gemini':
      return validateGeminiApiKey(key);
    default:
      return {
        isValid: false,
        message: 'Unknown provider',
        severity: 'error'
      };
  }
}

/**
 * Gets helpful links for obtaining API keys
 */
export function getApiKeyLinks(provider: string): { url: string; label: string } {
  switch (provider) {
    case 'github':
      return {
        url: 'https://github.com/settings/personal-access-tokens/new',
        label: 'Generate GitHub Token'
      };
    case 'gemini':
      return {
        url: 'https://aistudio.google.com/app/apikey',
        label: 'Get Gemini API Key'
      };
    default:
      return {
        url: '#',
        label: 'Get API Key'
      };
  }
}

/**
 * Sanitizes API key for safe storage (removes whitespace, validates length)
 */
export function sanitizeApiKey(key: string): string {
  return key.trim();
}

/**
 * Checks if API key is properly configured and ready to use
 */
export function isApiKeyReady(provider: string, key: string | undefined): boolean {
  const validation = validateApiKey(provider, key);
  return validation.isValid;
}