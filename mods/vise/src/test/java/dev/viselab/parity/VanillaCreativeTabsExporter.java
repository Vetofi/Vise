package dev.viselab.parity;

import com.google.gson.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import net.minecraft.ChatFormatting;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.world.flag.FeatureFlagSet;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;

public final class VanillaCreativeTabsExporter {
    public static void main(String[] args) throws Exception {
        ParityData.init();
        Files.writeString(Path.of(args[0]), export(FeatureFlags.DEFAULT_FLAGS, ParityData.lookup).toString(),
            StandardCharsets.UTF_8);
    }

    public static JsonObject export(FeatureFlagSet features, HolderLookup.Provider lookup) {
        CreativeModeTabs.tryRebuildTabContents(features, false, lookup);
        var without = new HashMap<CreativeModeTab, List<String>>();
        var visible = new HashMap<CreativeModeTab, Boolean>();
        for (CreativeModeTab tab : BuiltInRegistries.CREATIVE_MODE_TAB) {
            without.put(tab, tab.getDisplayItems().stream().map(VanillaCreativeTabsExporter::snbt).toList());
            visible.put(tab, tab.shouldDisplay());
        }
        CreativeModeTabs.tryRebuildTabContents(features, true, lookup);
        var tabs = new JsonArray();
        for (CreativeModeTab tab : BuiltInRegistries.CREATIVE_MODE_TAB) {
            var o = new JsonObject();
            var key = BuiltInRegistries.CREATIVE_MODE_TAB.getKey(tab);
            o.addProperty("id", key == null ? "" : key.toString());
            o.addProperty("type", tab.getType().name());
            o.addProperty("row", tab.row().name());
            o.addProperty("column", tab.column());
            o.addProperty("title", tab.getDisplayName().getString());
            o.addProperty("icon", snbt(tab.getIconItem()));
            o.addProperty("visible", visible.get(tab));
            var items = new JsonArray();
            var kept = new ArrayList<String>();
            var permissionsOnly = new JsonArray();
            var base = new HashSet<>(without.get(tab));
            for (ItemStack stack : tab.getDisplayItems()) {
                String s = snbt(stack);
                if (base.contains(s)) kept.add(s);
                else permissionsOnly.add(items.size());
                items.add(s);
            }
            if (!kept.equals(without.get(tab))) throw new IllegalStateException("permissions reorder " + key);
            o.add("items", items);
            if (!permissionsOnly.isEmpty()) o.add("permissions_only", permissionsOnly);
            if (tab.getType() == CreativeModeTab.Type.SEARCH) {
                var context = Item.TooltipContext.of(lookup);
                var flag = TooltipFlag.Default.NORMAL.asCreative();
                var lines = new JsonArray();
                var tags = new JsonArray();
                for (ItemStack stack : tab.getDisplayItems()) {
                    lines.add(strings(tooltipLines(stack, context, flag)));
                    tags.add(strings(stack.tags().map(t -> t.location().toString()).toList()));
                }
                o.add("lines", lines);
                o.add("tags", tags);
            }
            tabs.add(o);
        }
        var out = new JsonObject();
        out.addProperty("schema", 3);
        out.add("tabs", tabs);
        return out;
    }

    public static List<String> tooltipLines(ItemStack stack, Item.TooltipContext context, TooltipFlag flag) {
        return stack.getTooltipLines(context, null, flag).stream()
            .map(Component::getString)
            .map(l -> ChatFormatting.stripFormatting(l).trim())
            .filter(s -> !s.isEmpty())
            .toList();
    }

    public static String snbt(ItemStack stack) {
        if (stack.isEmpty()) return "-";
        return ItemStack.CODEC.encodeStart(ParityData.ops, stack).getOrThrow().toString();
    }

    private static JsonArray strings(List<String> values) {
        var a = new JsonArray();
        values.forEach(a::add);
        return a;
    }
}
