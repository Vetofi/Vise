package dev.viselab.parity;

import com.mojang.blaze3d.platform.NativeImage;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.Base64;
import net.minecraft.client.renderer.texture.SkinTextureDownloader;

public final class VanillaSkinImageOracle {
    public static void main(String[] args) throws Exception {
        Method process = SkinTextureDownloader.class.getDeclaredMethod("processLegacySkin", NativeImage.class, String.class);
        process.setAccessible(true);
        var out = new StringBuilder();
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            if (line.isBlank()) continue;
            String result;
            try {
                var image = (NativeImage) process.invoke(null, NativeImage.read(Base64.getDecoder().decode(line)), "case");
                var pixels = new byte[image.getWidth() * image.getHeight() * 4];
                for (int y = 0, i = 0; y < image.getHeight(); y++) {
                    for (int x = 0; x < image.getWidth(); x++) {
                        int argb = image.getPixel(x, y);
                        pixels[i++] = (byte) (argb >> 16);
                        pixels[i++] = (byte) (argb >> 8);
                        pixels[i++] = (byte) argb;
                        pixels[i++] = (byte) (argb >>> 24);
                    }
                }
                image.close();
                result = Base64.getEncoder().encodeToString(pixels);
            } catch (java.lang.reflect.InvocationTargetException e) {
                result = "refused";
            }
            out.append(result).append('\n');
        }
        Files.writeString(Path.of(args[1]), out.toString(), StandardCharsets.UTF_8);
    }
}
