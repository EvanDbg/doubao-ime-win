from PIL import Image, ImageDraw, ImageFilter
import os

def process_from_original(original_path, output_dir, final_size=48):
    """Process icons from the original combined image with better anti-aliasing"""
    os.makedirs(output_dir, exist_ok=True)
    
    # Load original
    img = Image.open(original_path).convert("RGBA")
    width, height = img.size
    
    # Split into 3 parts
    part_width = width // 3
    parts = [
        img.crop((0, 0, part_width, height)),
        img.crop((part_width, 0, part_width * 2, height)),
        img.crop((part_width * 2, 0, width, height))
    ]
    
    names = ["icon_idle.png", "icon_recording.png", "icon_processing.png"]
    
    for part, name in zip(parts, names):
        w, h = part.size
        
        # Crop center - keep 70% to preserve glow ring
        crop_ratio = 0.70
        margin_w = int(w * (1 - crop_ratio) / 2)
        margin_h = int(h * (1 - crop_ratio) / 2)
        cropped = part.crop((margin_w, margin_h, w - margin_w, h - margin_h))
        
        # Resize at 2x first for better anti-aliasing, then scale down
        large_size = final_size * 2
        cropped = cropped.resize((large_size, large_size), Image.Resampling.LANCZOS)
        
        # Create smooth circular mask at 2x resolution
        mask = Image.new("L", (large_size, large_size), 0)
        draw = ImageDraw.Draw(mask)
        # Draw circle with slight padding to avoid edge artifacts
        padding = 2
        draw.ellipse((padding, padding, large_size - padding, large_size - padding), fill=255)
        # Blur the mask edges for softer anti-aliasing
        mask = mask.filter(ImageFilter.GaussianBlur(radius=1))
        
        # Apply mask
        output_large = Image.new("RGBA", (large_size, large_size), (0, 0, 0, 0))
        output_large.paste(cropped, (0, 0), mask)
        
        # Scale down to final size with high quality
        output = output_large.resize((final_size, final_size), Image.Resampling.LANCZOS)
        
        output_path = os.path.join(output_dir, name)
        output.save(output_path, "PNG")
        print(f"Saved: {name} ({final_size}x{final_size}) with improved anti-aliasing")

if __name__ == "__main__":
    process_from_original("Gemini_Generated_Image_nse4abnse4abnse4.png", "assets", 48)
    print("Done!")
