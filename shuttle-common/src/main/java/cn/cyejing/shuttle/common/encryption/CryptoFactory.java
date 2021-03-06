package cn.cyejing.shuttle.common.encryption;

import cn.cyejing.shuttle.common.encryption.impl.AesCrypto;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.lang.reflect.Constructor;
import java.util.Collection;
import java.util.HashMap;
import java.util.Map;

public class CryptoFactory {

    private static final Logger logger = LoggerFactory.getLogger(CryptoFactory.class);

    private static final Map<String, String> crypts = new HashMap<>();

    static {
        crypts.putAll(AesCrypto.getCiphers());
    }

    public static Crypto get(String name, String password) {
        String className = crypts.get(name);
        if (className == null) {
            return null;
        }

        try {
            Class<?> clazz = Class.forName(className);
            Constructor<?> constructor = clazz.getConstructor(String.class,
                    String.class);
            return (Crypto) constructor.newInstance(name, password);
        } catch (Exception e) {
            logger.error("get crypt error", e);
        }

        return null;
    }

    public static boolean legalName(String name) {
        return crypts.containsKey(name);
    }

    public static Collection<String> supportName() {
        return crypts.keySet();
    }
}
